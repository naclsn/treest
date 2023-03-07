/// A `CommandGraph` is a tree of `Action`s (where `Immediate` are
/// leafs). An `Action` can be just a StaticCommand (`Fn`), potentially
/// with bound arguments (`Bind`) or a `Chain` of `Action` applied
/// one after the other. Finally a `StaticCommand` is a rust `fn`
/// (the actual action) with some meta (such as name/doc/completion).
use crate::{
    app::App,
    completions::Completer,
    line::{split_line_args, Message},
    node::{Filtering, Sorting, SortingProp},
    view::ScanToChoice,
};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use dirs::home_dir;
use glob::Pattern;
use lazy_static::lazy_static;
use std::{
    collections::HashMap, default::Default, fmt, fs, io, process::Command as SysCommand,
    str::FromStr,
};
use tui::layout::Direction;

#[derive(Debug, Clone)]
pub enum Action {
    Fn(&'static StaticCommand),
    Bind(&'static StaticCommand, Vec<String>),
    Chain(Vec<Action>),
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Action::Fn(sc) => write!(f, "{}", sc.name),
            Action::Bind(sc, bound) => write!(f, "{} {}", sc.name, bound.join(" ")),
            Action::Chain(v) => write!(
                f,
                "[{}]",
                v.iter()
                    .map(|a| a.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ), // TODO: better
        }
    }
}

impl Action {
    pub fn apply(&self, app: App, args: &[&str]) -> App {
        match self {
            Action::Fn(sc) => sc.action(app, args),
            Action::Bind(sc, bound) => {
                let mut v: Vec<_> = bound.iter().map(String::as_str).collect();
                v.extend_from_slice(args);
                sc.action(app, &v)
            }
            Action::Chain(acts) => acts
                .iter()
                .skip(1)
                .fold(acts[0].apply(app, args), |acc, cur| cur.apply(acc, &[])),
        }
    }

    pub fn get_comp(
        &self,
        args: &[&str],
        arg_idx: usize,
        ch_idx: usize,
        lookup: &impl Fn(&str) -> String,
    ) -> Vec<String> {
        match self {
            Action::Fn(sc) => sc.get_comp(args, arg_idx, ch_idx, lookup),
            Action::Bind(sc, bound) => {
                let mut v: Vec<_> = bound.iter().map(String::as_str).collect();
                v.extend_from_slice(args);
                sc.get_comp(&v, bound.len() + arg_idx, ch_idx, lookup)
            }
            Action::Chain(acts) => acts[0].get_comp(args, arg_idx, ch_idx, lookup),
        }
    }
}

#[derive(Hash, PartialEq, Eq, Debug, Clone, Copy)]
pub struct Key {
    kch: KeyCode,
    kmod: KeyModifiers,
}

mod key_names {
    pub const SPACE: &str = "space";
    pub const LESS_THAN: &str = "lt";
    pub const GREATER_THAN: &str = "gt";
    pub const BACKSPACE: &str = "backspace";
    pub const ENTER: &str = "enter";
    pub const LEFT: &str = "left";
    pub const RIGHT: &str = "right";
    pub const UP: &str = "up";
    pub const DOWN: &str = "down";
    pub const HOME: &str = "home";
    pub const END: &str = "end";
    pub const PAGEUP: &str = "pageup";
    pub const PAGEDOWN: &str = "pagedown";
    pub const TAB: &str = "tab";
    pub const DELETE: &str = "delete";
    pub const INSERT: &str = "insert";
}

// ZZZ: might be temporary
impl From<char> for Key {
    fn from(ch: char) -> Key {
        Key {
            kch: KeyCode::Char(ch),
            kmod: if ch.is_ascii_uppercase() {
                KeyModifiers::SHIFT
            } else {
                KeyModifiers::NONE
            },
        }
    }
}

impl From<&KeyEvent> for Key {
    fn from(ev: &KeyEvent) -> Key {
        Key {
            kch: ev.code,
            kmod: ev.modifiers,
        }
    }
}

impl FromStr for Key {
    type Err = (); // ZZZ: fleme

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut chs = s.chars();
        match chs.next() {
            Some('<') => {
                let mut chs = chs.peekable();
                let mut accmod = KeyModifiers::NONE;

                while let Some(&m @ ('S' | 'A' | 'C')) = chs.peek() {
                    chs.next();
                    if let Some('-') = chs.next() {
                        accmod.insert(match m {
                            'S' => KeyModifiers::SHIFT,
                            'A' => KeyModifiers::ALT,
                            'C' => KeyModifiers::CONTROL,
                            _ => unreachable!(),
                        });
                    } else {
                        return Err(());
                    }
                }

                let name = chs.take_while(|ch| '>' != *ch).collect::<String>();
                Ok(Key {
                    kch: match name.as_str() {
                        h if 1 == h.len() => KeyCode::Char(h.as_bytes()[0] as char),
                        h if 1 < h.len() && b'F' == h.as_bytes()[0] => {
                            KeyCode::F(h[1..].parse().unwrap())
                        }
                        key_names::SPACE => KeyCode::Char(' '),
                        key_names::LESS_THAN => KeyCode::Char('<'),
                        key_names::GREATER_THAN => KeyCode::Char('>'),
                        key_names::BACKSPACE => KeyCode::Backspace,
                        key_names::ENTER => KeyCode::Enter,
                        key_names::LEFT => KeyCode::Left,
                        key_names::RIGHT => KeyCode::Right,
                        key_names::UP => KeyCode::Up,
                        key_names::DOWN => KeyCode::Down,
                        key_names::HOME => KeyCode::Home,
                        key_names::END => KeyCode::End,
                        key_names::PAGEUP => KeyCode::PageUp,
                        key_names::PAGEDOWN => KeyCode::PageDown,
                        key_names::TAB => KeyCode::Tab,
                        key_names::DELETE => KeyCode::Delete,
                        key_names::INSERT => KeyCode::Insert,
                        _ => {
                            // YYY: weird(er) keys not handled on purpose
                            return Err(());
                        }
                    },
                    kmod: accmod,
                })
            }

            Some('F') => Ok(Key {
                kch: KeyCode::F(chs.collect::<String>().parse().unwrap()),
                kmod: KeyModifiers::NONE,
            }),

            Some(ch) => Ok(ch.into()),

            None => Err(()),
        }
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut hasmod = false;
        let mut putmod = |f: &mut fmt::Formatter<'_>| -> fmt::Result {
            if !hasmod {
                write!(f, "<")?;
                hasmod = true;
            }
            Ok(())
        };

        if self.kmod.contains(KeyModifiers::SHIFT) {
            match self.kch {
                KeyCode::Char(ch) if ch.is_ascii_uppercase() => (),
                _ => {
                    putmod(f)?;
                    write!(f, "S-")?;
                }
            }
        }
        if self.kmod.contains(KeyModifiers::ALT) {
            putmod(f)?;
            write!(f, "A-")?;
        }
        if self.kmod.contains(KeyModifiers::CONTROL) {
            putmod(f)?;
            write!(f, "C-")?;
        }

        match self.kch {
            KeyCode::Char(' ') => {
                putmod(f)?;
                f.write_str(key_names::SPACE)
            }
            KeyCode::Char('<') => {
                putmod(f)?;
                f.write_str(key_names::LESS_THAN)
            }
            KeyCode::Char('>') => {
                putmod(f)?;
                f.write_str(key_names::GREATER_THAN)
            }

            KeyCode::Char(ch) => write!(f, "{ch}"),

            KeyCode::F(n) => write!(f, "F{n}"),

            other => {
                putmod(f)?;
                match other {
                    KeyCode::Backspace => f.write_str(key_names::BACKSPACE),
                    KeyCode::Enter => f.write_str(key_names::ENTER),
                    KeyCode::Left => f.write_str(key_names::LEFT),
                    KeyCode::Right => f.write_str(key_names::RIGHT),
                    KeyCode::Up => f.write_str(key_names::UP),
                    KeyCode::Down => f.write_str(key_names::DOWN),
                    KeyCode::Home => f.write_str(key_names::HOME),
                    KeyCode::End => f.write_str(key_names::END),
                    KeyCode::PageUp => f.write_str(key_names::PAGEUP),
                    KeyCode::PageDown => f.write_str(key_names::PAGEDOWN),
                    KeyCode::Tab => f.write_str(key_names::TAB),
                    KeyCode::Delete => f.write_str(key_names::DELETE),
                    KeyCode::Insert => f.write_str(key_names::INSERT),
                    _ => Ok(()), // YYY: weird(er) keys not handled on purpose
                }
            }
        }?;

        if hasmod {
            write!(f, ">")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
enum CommandGraph {
    Immediate(Action),
    Pending(HashMap<Key, CommandGraph>),
}

#[derive(Debug, Clone)]
pub struct CommandMap(HashMap<Key, CommandGraph>);

fn display_graph_keymap(
    map: &HashMap<Key, CommandGraph>,
    f: &mut fmt::Formatter<'_>,
) -> fmt::Result {
    let depth = f.precision().unwrap_or(0);
    let indent = "   ".repeat(depth);
    write!(
        f,
        "{}",
        map.iter()
            .map(|(k, c)| format!("{k}  {c:.*}", depth + 1))
            .collect::<Vec<_>>()
            .join(&format!("\n{indent}"))
    )
}
impl fmt::Display for CommandGraph {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandGraph::Immediate(act) => write!(f, "{act}"),
            CommandGraph::Pending(map) => display_graph_keymap(map, f),
        }
    }
}

impl fmt::Display for CommandMap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        display_graph_keymap(&self.0, f)
    }
}

macro_rules! make_map_one {
    ($key:expr => [$(($key2:literal, $value:tt),)*]) => {
        ($key, CommandGraph::Pending(make_map!($(($key2, $value),)*)))
    };
    ($key:expr => $action:tt) => {
        ($key, CommandGraph::Immediate(make_map_one!(@ $action)))
    };
    (@ $sc:ident) => {
        Action::Fn(&$sc)
    };
    (@ ($sc:ident, $($bound:literal),+)) => {
        Action::Bind(&$sc, vec![$($bound.to_string()),+])
    };
    (@ ($($actions:tt),*)) => {
        Action::Chain(vec![$(make_map_one!(@ $actions)),*])
    };
    (@ $action:expr) => {
        $action // YYY: probably should not enable this
    };
}

macro_rules! make_map {
    ($(($key:literal, $value:tt),)*) => {
        HashMap::from([
            $(make_map_one!($key.into() => $value),)*
        ])
    };
}

impl Default for CommandMap {
    fn default() -> CommandMap {
        CommandMap(make_map![
            ('q', quit),
            ('Q', quit),
            (
                'w',
                [
                    ('s', (split, "horizontal")),
                    ('v', (split, "vertical")),
                    ('t', transpose_splits),
                    ('q', close_split),
                    ('o', close_other_splits),
                    ('w', to_view_next),
                    ('W', to_view_prev),
                    ('h', (to_view, "right")),
                    ('l', (to_view, "left")),
                    ('j', (to_view, "down")),
                    ('k', (to_view, "up")),
                    // ('h', move_view_right),
                    // ('l', move_view_left),
                    // ('j', move_view_down),
                    // ('k', move_view_up),
                ]
            ),
            ('?', help),
            (':', command),
            ('/', (prompt, "/", "find_in")),
            ('H', fold_node),
            ('L', unfold_node),
            ('h', leave_node),
            ('l', enter_node),
            ('j', next_node),
            ('k', prev_node),
            (' ', (toggle_marked, next_node)),
            (
                'g',
                [('g', {
                    Action::Fn(&StaticCommand {
                        name: "",
                        doc: "",
                        action: |mut app: App, _| {
                            app.focused_mut().cursor_to_root();
                            app
                        },
                        comp: Completer::None,
                    })
                }),]
            ),
            ('y', (scroll_view, "-3")),
            ('e', (scroll_view, "3")),
            ('Y', (shift_view, "-3")),
            ('E', (shift_view, "3")),
            (':', (prompt, ":", "command")),
            ('!', (prompt, "!", "shell")),
            ('<', (prompt, "<", "echo")),
            ('>', (prompt, ">", "read")),
        ])
    }
}

impl CommandMap {
    pub fn try_get_action(&self, key_path: &[Key]) -> (Option<&Action>, bool) {
        if key_path.is_empty() {
            return (None, true);
        }

        let mut curr = self.0.get(&key_path[0]);
        for key in &key_path[1..] {
            match curr {
                None => return (None, false),
                Some(CommandGraph::Immediate(action)) => return (Some(action), false),
                Some(CommandGraph::Pending(next)) => curr = next.get(key),
            }
        }

        if let Some(CommandGraph::Immediate(action)) = curr {
            (Some(action), false)
        } else {
            (None, curr.is_some())
        }
    }

    pub fn rebind(&mut self, key_path: &[Key], action: Action) {
        let mut acc = &mut self.0;
        for ch in &key_path[..key_path.len() - 1] {
            acc = {
                match acc.get(ch) {
                    Some(CommandGraph::Pending(_)) => {
                        let Some(CommandGraph::Pending(next)) = acc.get_mut(ch) else { unreachable!(); };
                        next
                    }
                    Some(CommandGraph::Immediate(_)) | None => {
                        let niw = HashMap::new();
                        acc.insert(*ch, CommandGraph::Pending(niw));
                        let Some(CommandGraph::Pending(niw)) = acc.get_mut(ch) else { unreachable!(); };
                        niw
                    }
                }
            };
        }
        acc.insert(
            key_path[key_path.len() - 1],
            CommandGraph::Immediate(action),
        );
    }
}

pub struct StaticCommand {
    name: &'static str,
    doc: &'static str,
    action: fn(App, &[&str]) -> App,
    comp: Completer,
}

impl fmt::Debug for StaticCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StaticCommand")
            .field("name", &self.name)
            .field("doc", &self.doc)
            .finish()
    }
}

impl StaticCommand {
    fn action(&self, app: App, args: &[&str]) -> App {
        let f = self.action;
        f(app, args)
    }

    pub fn get_comp(
        &self,
        args: &[&str],
        arg_idx: usize,
        ch_idx: usize,
        lookup: &impl Fn(&str) -> String,
    ) -> Vec<String> {
        self.comp.get_comp(args, arg_idx, ch_idx, lookup)
    }

    pub fn get_comp_itself(&self) -> &Completer {
        &self.comp
    }
}

macro_rules! make_lst {
    ($($name:ident = ($doc:literal, $action:expr, $comp:expr,);)*) => {
        $(
            #[allow(non_upper_case_globals)]
            pub const $name: StaticCommand = StaticCommand {
                name: stringify!($name),
                doc: $doc,
                action: $action,
                comp: $comp,
            };
        )*

        pub const COMMAND_LIST: &'static[&'static str] = &[$(stringify!($name),)*];

        lazy_static! {
            pub static ref COMMAND_MAP: HashMap<&'static str, &'static StaticCommand> =
                HashMap::from([$((stringify!($name), &$name),)*]);
        }
    };
}

// YYY: for some reason it stopped formatting this macro when adding longer doc strings...
make_lst! {

    bind = (
        "bind a key, or sequence, to a command (arguments can be provided for the command)",
        |mut app: App, args: &[&str]| {
            if args.len() < 2 {
                app.message(Message::Warning(
                    "bind needs a key, a command name and optional arguments".to_string(),
                ));
                return app;
            }
            let key_path = args[0]
                .split_whitespace()
                .map(|s| s.parse().unwrap())
                .collect::<Vec<_>>();
            let Some(sc) = COMMAND_MAP.get(args[1]) else {
                app.message(Message::Warning(format!("cannot bind unknown command '{}'", args[1])));
                return app;
            };
            let bound = &args[2..];
            app.rebind(
                &key_path,
                Action::Bind(sc, bound.iter().map(|s| s.to_string()).collect()),
            );
            app
        },
        Completer::StaticNth(&[
            Completer::None,
            Completer::StaticWords(COMMAND_LIST),
            Completer::Defered(|args, _, _| {
                COMMAND_MAP
                    .get(args[1])
                    .map(|sc| Completer::Of(sc, 2))
                    .unwrap_or(Completer::None)
            }),
        ]),
    );

    bindings = (
        "list the current bindings",
        |mut app: App, _| {
            app.message(Message::Info({
                let b = app.get_bindings();
                format!("{b}")
            }));
            app
        },
        Completer::None,
    );

    close_other_splits = (
        "close every other views, keeping only the current one",
        |mut app: App, _| {
            app.view_close_other();
            app
        },
        Completer::None,
    );

    close_split = (
        "close the current focused view",
        |mut app: App, _| {
            app.view_close();
            app
        },
        Completer::None,
    );

    command = (
        "execute a command, passing the rest of the arguments",
        |mut app: App, args: &[&str]| {
            if args.is_empty() {
                app.message(Message::Warning(
                    "expand needs a command and an argument string".to_string(),
                ));
                return app;
            }
            if let Some(com) = COMMAND_MAP.get(args[0]) {
                let action = com.action;
                action(app, &args[1..])
            } else {
                app.message(Message::Warning(format!("unknown command: '{}'", args[0])));
                app
            }
        },
        Completer::StaticNth(&[
            Completer::StaticWords(COMMAND_LIST),
            Completer::Defered(|args, _, _| {
                COMMAND_MAP
                    .get(args[0])
                    .map(|sc| Completer::Of(sc, 1))
                    .unwrap_or(Completer::None)
            }),
        ]),
    );

    declare = (
        "declare a variable, giving it a value",
        |mut app: App, args: &[&str]| {
            if args.is_empty() {
                app.message(Message::Warning(
                    "declare needs a name and optional value".to_string(),
                ));
                return app;
            }
            let name = args[0];
            let value = args.get(1).unwrap_or(&"");
            app.declare(name, value);
            app
        },
        Completer::None,
    );

    echo = (
        "echo the arguments to standard output, usually to be captured by the calling process (eg. in shell script)",
        |app: App, args: &[&str]| {
            let mut first = true;
            for it in args {
                if first {
                    first = false;
                } else {
                    print!(" ");
                }
                if let Some((before, rest)) = it.split_once('$') {
                    print!("{before}");
                    if let Some(stripped) = rest.strip_prefix('{') {
                        if let Some((name, rest)) = stripped.split_once('}') {
                            print!("{}{rest}", app.lookup(name));
                        } else {
                            print!("{rest}")
                        }
                    } else {
                        let mut chs = it.chars();
                        let name = chs
                            .by_ref()
                            .take_while(|c| c.is_ascii_alphanumeric())
                            .collect::<String>();
                        let rest = chs.collect::<String>();
                        print!("{}{rest}", app.lookup(&name));
                    }
                } else {
                    print!("{it}");
                }
            }
            println!();
            app
        },
        Completer::None,
    );

    enter_node = (
        "try to enter the node at the cursor, unfolding it first",
        |mut app: App, _| {
            let (view, tree) = app.focused_and_tree_mut();
            if view.unfold(tree).is_ok() {
                view.enter();
            }
            app
        },
        Completer::None,
    );

    expand = (
        "execute a command, expanding the second argument into its arguments",
        |mut app: App, args: &[&str]| {
            if args.len() < 2 {
                app.message(Message::Warning(
                    "expand needs a command and an argument string".to_string(),
                ));
                return app;
            }
            if let Some(com) = COMMAND_MAP.get(args[0]) {
                let action = com.action;
                let args = split_line_args(args[1], &|name| app.lookup(name));
                action(app, &args.iter().map(String::as_ref).collect::<Vec<_>>())
            } else {
                app.message(Message::Warning(format!("unknown command: '{}'", args[0])));
                app
            }
        },
        Completer::StaticWords(COMMAND_LIST),
    );

    filter = (
        "add or remove filters",
        |mut app: App, args: &[&str]| {
            let (view, tree) = app.focused_and_tree_mut();
            match args {
                [does @ ("add" | "remove"), what, ..] => {
                    let f = match *what {
                        "git" => Filtering::new_git(),
                        "pattern" => Filtering::new_pattern({
                            if let Some(pat) = args.get(2) {
                                pat.to_string()
                            } else {
                                app.message(Message::Warning("missing filter pattern".to_string()));
                                return app;
                            }
                        }),
                        "dotfiles" => Filtering::new_pattern(".*".to_string()),
                        incorrect => {
                            app.message(Message::Warning(format!(
                                "incorrect filter type: {incorrect:?}"
                            )));
                            return app;
                        }
                    };
                    match *does {
                        "add" => view.add_filtering(f),
                        "remove" => view.remove_filtering(f),
                        _ => unreachable!(),
                    }
                    view.renew_root(tree, tree).unwrap();
                }
                ["clear", ..] => {
                    view.clear_filtering();
                    view.renew_root(tree, tree).unwrap();
                }
                ["list", ..] => {
                    let l = view.list_filtering();
                    if l.is_empty() {
                        app.message(Message::Info("# (no filter)".to_string()));
                    } else {
                        let s = l
                            .iter()
                            .map(|it| format!("{it}"))
                            .collect::<Vec<_>>()
                            .join("\n");
                        app.message(Message::Info(format!("# filters:\n{s}")));
                    }
                }
                [incorrect, ..] => {
                    app.message(Message::Warning(format!(
                        "incorrect action for filter: {incorrect:?}"
                    )));
                    return app;
                }
                [] => {
                    app.message(Message::Warning(
                        "filter needs a action and an optional argument".to_string(),
                    ));
                    return app;
                }
            };
            app
        },
        Completer::StaticNth(&[
            Completer::StaticWords(&["add", "remove", "clear", "list"]),
            Completer::StaticWords(&["git", "pattern", "dotfiles"]),
            Completer::None,
        ]),
    );

    find = (
        "find a node, which name contains the search string, in the folder at the currsor",
        |mut app: App, args: &[&str]| {
            if args.is_empty() {
                app.message(Message::Warning(
                    "find needs a search string".to_string()
                ));
                return app;
            }
            let search = args[0];
            let (view, tree) = app.focused_and_tree_mut();
            view.scan_to(tree, &|_, node| {
                if node.file_name().contains(search) {
                    ScanToChoice::Break(false)
                } else {
                    ScanToChoice::Continue(false)
                }
            });
            app
        },
        Completer::FileFromCursor,
    );

    find_and_toggle_marked = (
        "find nodes matching the pattern and mark/unmark them, in the folder at the cursor",
        |mut app: App, args: &[&str]| {
            if args.is_empty() {
                app.message(Message::Warning(
                    "find_and_toggle_marked needs a search string".to_string(),
                ));
                return app;
            }
            let Ok(search) = Pattern::new(args[0]) else {
                app.message(Message::Warning(
                    format!("error parsing the pattern '{}'", args[0])
                ));
                return app;
            };
            let (view, tree) = app.focused_and_tree_mut();
            view.scan_to(tree, &|state, node| {
                ScanToChoice::Continue(state.marked != search.matches(node.file_name()))
            });
            app
        },
        Completer::FileFromCursor,
    );

    find_in = (
        "find a node, which name contains the search string, in the folder the cursor is in",
        |mut app: App, args: &[&str]| {
            if args.is_empty() {
                app.message(Message::Warning(
                    "find_in needs a search string".to_string()
                ));
                return app;
            }
            let search = args[0];
            app.focused_mut().leave();
            let (view, tree) = app.focused_and_tree_mut();
            let found = view.scan_to(tree, &|_, node| {
                if node.file_name().contains(search) {
                    ScanToChoice::Break(false)
                } else {
                    ScanToChoice::Continue(false)
                }
            });
            if !found {
                app.focused_mut().enter();
            }
            app
        },
        Completer::FileFromCursor,
    );

    fold_node = (
        "fold the node at the cursor",
        |mut app: App, _| {
            app.focused_mut().fold();
            app
        },
        Completer::None,
    );

    help = (
        "get help for a command or a list of known commands",
        |mut app: App, args: &[&str]| {
            if let Some(name) = args.first() {
                if let Some(com) = COMMAND_MAP.get(name) {
                    app.message(Message::Info(format!(
                        "{}: {} -- completion:\n{:.1}",
                        com.name, com.doc, com.comp
                    )));
                } else {
                    app.message(Message::Warning(format!("unknown command name '{name}'")));
                }
            } else {
                app.message(Message::Info(
                    COMMAND_MAP.keys().copied().collect::<Vec<_>>().join(" "),
                ));
            }
            app
        },
        Completer::StaticWords(COMMAND_LIST),
    );

    keys = (
        "send key events as if theses where pressed by the user (whatever you are doing, this should really be last resort)",
        |app: App, args: &[&str]| {
            args.iter().fold(app, |app, ks| {
                ks.split_whitespace()
                    .map(|s| s.parse().unwrap())
                    .fold(app, |app, k: Key| {
                        app.do_event(&Event::Key(KeyEvent::new(k.kch, k.kmod)))
                    })
            })
        },
        Completer::None,
    );

    leave_node = (
        "move the cursor to the parent node",
        |mut app: App, _| {
            app.focused_mut().leave();
            app
        },
        Completer::None,
    );

    message = (
        "display a message, level should be one of info/warning/error",
        |mut app: App, args: &[&str]| {
            match args.first() {
                Some(&"info") => app.message(Message::Info(args[1..].join(" "))),
                Some(&"warning") => app.message(Message::Warning(args[1..].join(" "))),
                Some(&"error") => app.message(Message::Error(args[1..].join(" "))),
                _ => app.message(Message::Warning(
                    "message needs a level and messages".to_string(),
                )),
            }
            app
        },
        Completer::StaticWords(&["info", "warning", "error"]),
    );

    next_node = (
        "move the cursor to the next node",
        |mut app: App, _| {
            app.focused_mut().next();
            app
        },
        Completer::None,
    );

    prev_node = (
        "move the cursor to the previous node",
        |mut app: App, _| {
            app.focused_mut().prev();
            app
        },
        Completer::None,
    );

    prompt = (
        "prompt for input, then execute a command with it (the first argument should be the prompt text)",
        |mut app: App, args: &[&str]| {
            if args.len() < 2 {
                app.message(Message::Warning("prompt needs a prompt text, a command name and optional arguments".to_string()));
                return app;
            }
            if let Some(then) = COMMAND_MAP.get(args[1]) {
                app.prompt(args[0].to_string(), Action::Bind(then, args[2..].iter().map(|s| s.to_string()).collect()), None);
            } else {
                app.message(Message::Warning(format!("unknown command '{}'", args[1])));
            }
            app
        },
        Completer::StaticNth(&[
            Completer::None,
            Completer::StaticWords(COMMAND_LIST),
            Completer::Defered(|args, _, _| {
                COMMAND_MAP.get(args[1])
                    .map(|sc| Completer::Of(sc, 2))
                    .unwrap_or(Completer::None)
            }),
        ]),
    );

    prompt_init = (
        "prompt for input with an initial value, then execute a command with it (the first argument should be the prompt text)",
        |mut app: App, args: &[&str]| {
            if args.len() < 3 {
                app.message(Message::Warning("prompt_init needs a prompt text, an initial value, a command name and optional arguments".to_string()));
                return app;
            }
            if let Some(then) = COMMAND_MAP.get(args[2]) {
                app.prompt(args[0].to_string(), Action::Bind(then, args[3..].iter().map(|s| s.to_string()).collect()), Some(args[1]));
            } else {
                app.message(Message::Warning(format!("unknown command '{}'", args[2])));
            }
            app
        },
        Completer::StaticNth(&[
            Completer::None,
            Completer::None,
            Completer::StaticWords(COMMAND_LIST),
            Completer::Defered(|args, _, _| {
                COMMAND_MAP.get(args[2])
                    .map(|sc| Completer::Of(sc, 3))
                    .unwrap_or(Completer::None)
            }),
        ]),
    );

    quit = (
        "save the state and quit with successful exit status",
        |mut app: App, _| {
            app.finish();
            app
        },
        Completer::None,
    );

    read = (
        "read, into the variables with given name, from standard input, usually sent from the calling process (eg. in shell script)",
        |mut app: App, args: &[&str]| {
            let mut line = String::new();
            let Ok(_) = io::stdin().read_line(&mut line) else { return app; };
            let mut values = line.split_whitespace();
            for it in args {
                app.declare(it, values.next().unwrap_or(""));
            }
            app
        },
        Completer::None,
    );

    reload = (
        "reload the tree from the file system",
        |mut app: App, _| {
            let (view, tree) = app.focused_and_tree_mut();
            let ntree = tree.renew().unwrap();
            view.renew_root(tree, &ntree).unwrap();
            app.tree = ntree;
            app
        },
        Completer::None,
    );

    scroll_view = (
        "scroll the focused view (vertically)",
        |mut app: App, args: &[&str]| {
            let by = args.first().map_or(Ok(1), |n| n.parse()).unwrap_or(1);
            app.focused_mut().view_offset().scroll += by;
            app
        },
        Completer::None,
    );

    seq = (
        "execute a sequence of commands",
        |mut app: App, comms: &[&str]| {
            let idk = comms
                .iter()
                .map(|l| split_line_args(l, &|name| app.lookup(name)))
                .collect::<Vec<_>>();
            for com0_args in idk {
                let (com0, args) = com0_args.split_at(1);
                let Some(act) = COMMAND_MAP.get(com0[0].as_str()).map(|c| c.action) else {
                    app.message(Message::Warning(format!("unknown command: {}", com0[0])));
                    return app;
                };
                app = act(app, &args.iter().map(String::as_str).collect::<Vec<_>>());
            }
            app
        },
        Completer::None,
    );

    shell = (
        "execute a shell command for its output, passing the rest as arguments",
        |mut app: App, args: &[&str]| {
            match args {
                [h, t @ ..] => match SysCommand::new(h).args(t).output() {
                    Ok(res) => {
                        if res.status.success() {
                            app.message(Message::Info(
                                String::from_utf8_lossy(&res.stdout).to_string(),
                            ));
                        } else {
                            app.message(Message::Warning(
                                String::from_utf8_lossy(&res.stderr).to_string(),
                            ));
                        }
                    }
                    Err(err) => {
                        app.message(Message::Error(format!("{err}")));
                    }
                },
                _ => app.message(Message::Warning(
                    "shell needs an executable name and optional arguments".to_string(),
                )),
            }
            app
        },
        Completer::StaticNth(&[Completer::PathLookup, Completer::FileFromRoot]),
    );

    shell_wait = (
        "execute a shell command and wait for it to finish, passing the rest as arguments",
        |mut app: App, args: &[&str]| {
            match args {
                [h, t @ ..] => {
                    let h = h.to_string();
                    let t = t.iter().map(|s| s.to_string()).collect::<Vec<_>>();
                    app.execute_in_restored(Box::new(|mut app| {
                        match SysCommand::new(h).args(t).status() {
                            Ok(res) => {
                                if res.success() {
                                    app.message(Message::Info("exited successfully".to_string()));
                                } else {
                                    app.message(Message::Warning(if let Some(code) = res.code() {
                                        format!("exited with status '{code:?}'")
                                    } else {
                                        "exited with a failure (no exit code)".to_string()
                                    }));
                                }
                            }
                            Err(err) => {
                                app.message(Message::Error(format!("{err}")));
                            }
                        }
                        app
                    }));
                }
                _ => app.message(Message::Warning(
                    "shell needs an executable name and optional arguments".to_string(),
                )),
            }
            app
        },
        Completer::StaticNth(&[Completer::PathLookup, Completer::FileFromRoot]),
    );

    shift_view = (
        "shift the focused view (horizontally)",
        |mut app: App, args: &[&str]| {
            let by = args.first().map_or(Ok(1), |n| n.parse()).unwrap_or(1);
            app.focused_mut().view_offset().shift += by;
            app
        },
        Completer::None,
    );

    sort = (
        "change the way nodes are sorted for the focused view",
        |mut app: App, args: &[&str]| {
            let prop = match args.first() {
                Some(&"none") => SortingProp::None,
                Some(&"name") => SortingProp::Name,
                Some(&"size") => SortingProp::Size,
                Some(&"extension") => SortingProp::Extension,
                Some(&"atime") => SortingProp::ATime,
                Some(&"mtime") => SortingProp::MTime,
                Some(&"ctime") => SortingProp::CTime,
                Some(unk) => {
                    app.message(Message::Warning(format!(
                        "cannot sort by unknown property '{unk}'"
                    )));
                    return app;
                }
                None => {
                    app.message(Message::Warning(
                        "sort needs a propery to sort by".to_string(),
                    ));
                    return app;
                }
            };
            let mut skip = 1;
            let (view, tree) = app.focused_and_tree_mut();
            view.set_sorting(
                Sorting::new(
                    prop,
                    match args.get(skip) {
                        Some(&"dirs_first") => {
                            skip += 1;
                            true
                        }
                        _ => false,
                    },
                ),
                match args.get(skip) {
                    Some(&"reverse") => {
                        // skip += 1;
                        true
                    }
                    _ => false,
                },
            );
            // (tree not changed, hence same)
            view.renew_root(tree, tree).unwrap();
            // if let Some(rest) = args.get(skip) {
            //     app.message(Message::Warning(format!(
            //         "unknown extraneous argument '{rest}'"
            //     )));
            // }
            app
        },
        Completer::StaticNth(&[
            Completer::StaticWords(&[
                "none",
                "name",
                "size",
                "extension",
                "atime",
                "mtime",
                "ctime",
            ]),
            Completer::StaticWords(&["dirs_first", "reverse"]),
            Completer::StaticWords(&["reverse"]),
        ]),
    );

    source = (
        "source the given file, executing each line as a command",
        |mut app: App, args: &[&str]| {
            if args.is_empty() {
                app.message(Message::Warning("source needs a file path".to_string()));
                return app;
            }
            let filename = if let Some(suffix) = args[0].strip_prefix("~/") {
                let mut r = suffix.to_string();
                r.insert(0, '/');
                r.insert_str(0, home_dir().unwrap().to_string_lossy().as_ref());
                r
            } else {
                args[0].to_string()
            };
            let res = fs::read_to_string(&filename);
            match res {
                Err(err) => app.message(Message::Warning(format!(
                    "could not read file {filename}: {err}"
                ))),
                Ok(content) => {
                    for (k, com0_args) in content
                        .lines()
                        .map(|l| split_line_args(l, &|name| format!("{{{name}}}"))) // interpolation in not enabled when sourcing
                        .enumerate()
                        .filter(|(_, v)| !v.is_empty())
                    {
                        let (com0, args) = com0_args.split_at(1);
                        let Some(act) = COMMAND_MAP.get(com0[0].as_str()).map(|c| c.action) else {
                            app.message(Message::Warning(format!("unknown command at line {k}: {}", com0[0])));
                            return app;
                        };
                        app = act(app, &args.iter().map(String::as_str).collect::<Vec<_>>());
                    }
                    app.message(Message::Info(format!("successfully sourced {filename}")));
                } // Ok
            } // match
            app
        },
        Completer::FileFromRoot,
    );

    split = (
        "create a new split base on this view, horizontal or vertical",
        |mut app: App, args: &[&str]| match args.first() {
            Some(&"horizontal") => {
                app.view_split(Direction::Horizontal);
                app
            }
            Some(&"vertical") => {
                app.view_split(Direction::Vertical);
                app
            }
            _ => app,
        },
        Completer::StaticWords(&["horizontal", "vertical"]),
    );

    toggle_marked = (
        "mark/unmark the node at the cursor",
        |mut app: App, _| {
            app.focused_mut().toggle_marked();
            app
        },
        Completer::None,
    );

    to_view = (
        "move to a view right/left/down/up from the focused one",
        |mut app: App, args: &[&str]| {
            match args.first() {
                Some(&"right") => app.focus_to_view(Direction::Horizontal, -1),
                Some(&"left") => app.focus_to_view(Direction::Horizontal, 1),
                Some(&"down") => app.focus_to_view(Direction::Vertical, 1),
                Some(&"up") => app.focus_to_view(Direction::Vertical, -1),
                _ => (),
            }
            app
        },
        Completer::StaticWords(&["right", "left", "down", "up"]),
    );

    to_view_next = (
        "move to the next view in the same split",
        |mut app: App, _| {
            app.focus_to_view_adjacent(1);
            app
        },
        Completer::None,
    );

    to_view_prev = (
        "move to the previous view in the same split",
        |mut app: App, _| {
            app.focus_to_view_adjacent(-1);
            app
        },
        Completer::None,
    );

    transpose_splits = (
        "transpose the split containing the focused view",
        |mut app: App, _| {
            app.view_transpose();
            app
        },
        Completer::None,
    );

    unfold_node = (
        "try to unfold the node at the cursor (nothing happens if it is not a directory or a link to one)",
        |mut app: App, _| {
            let (view, tree) = app.focused_and_tree_mut();
            view.unfold(tree).ok();
            app
        },
        Completer::None,
    );

}
