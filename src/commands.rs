/// All this is quite confused, but here is the idea. A `CommandGraph`
/// is a tree of `Action`s (where `Immediate` are leafs). An `Action`
/// can be just a StaticCommand (`Fn`), potentially with bound arguments
/// (`Bind`) or a `Chain` of `Action` applied one after the other. Finally a
/// `StaticCommand` is a rust `fn` (the actual action) with some meta (such
/// as name/doc/completion).
use crate::{
    app::App,
    completions::Completer,
    line::{split_line_args, Message},
    node::{Sorting, SortingProp},
};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use lazy_static::lazy_static;
use std::{collections::HashMap, default::Default, fmt, fs, io, process::Command as SysCommand};
use tui::layout::Direction;

#[derive(Debug, Clone)]
pub enum Action {
    Fn(&'static StaticCommand),
    Bind(&'static StaticCommand, Vec<String>),
    Chain(Vec<Action>),
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

    pub fn get_comp(&self, args: &[&str], arg_idx: usize, ch_idx: usize) -> Vec<String> {
        match self {
            Action::Fn(sc) => sc.get_comp(args, arg_idx, ch_idx),
            Action::Bind(sc, bound) => {
                let mut v: Vec<_> = bound.iter().map(String::as_str).collect();
                v.extend_from_slice(args);
                sc.get_comp(&v, bound.len() + arg_idx, ch_idx)
            }
            Action::Chain(acts) => acts[0].get_comp(args, arg_idx, ch_idx),
        }
    }
}

#[derive(Debug, Clone)]
enum CommandGraph {
    Immediate(Action),
    Pending(HashMap<char, CommandGraph>),
}

#[derive(Debug, Clone)]
pub struct CommandMap(HashMap<char, CommandGraph>);

macro_rules! make_map_one {
    ($key:literal => [$(($key2:literal, $value:tt),)*]) => {
        ($key, CommandGraph::Pending(make_map!($(($key2, $value),)*)))
    };
    ($key:literal => $action:tt) => {
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
            $(make_map_one!($key => $value),)*
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

// TODO: handle mouse events and modifier keys
impl CommandMap {
    pub fn try_get_action(&self, key_path: &[char]) -> (Option<&Action>, bool) {
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
            return (Some(action), false);
        } else {
            return (None, curr.is_some());
        }
    }

    pub fn rebind(&mut self, key_path: &[char], action: Action) {
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

    pub fn get_comp(&self, args: &[&str], arg_idx: usize, ch_idx: usize) -> Vec<String> {
        self.comp.get_comp(args, arg_idx, ch_idx)
    }
}

macro_rules! make_lst {
    ($($name:ident = ($doc:literal, $action:expr, $comp:expr),)*) => {
        $(
            #[allow(non_upper_case_globals)]
            pub static $name: StaticCommand = StaticCommand {
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
make_lst!(
    help = (
        "get help for a command or a list of known commands",
        |mut app: App, args: &[&str]| {
            if let Some(name) = args.get(0) {
                if let Some(com) = COMMAND_MAP.get(name) {
                    app.message(Message::Info(format!("{}: {}\n{:.1}", com.name, com.doc, com.comp)));
                } else {
                    app.message(Message::Warning(format!("unknown command name '{name}'")));
                }
            } else {
                app.message(Message::Info(COMMAND_MAP.keys().map(|s| *s).collect::<Vec<_>>().join(" ")));
            }
            app
        },
        Completer::StaticWords(COMMAND_LIST)
    ),
    command = (
        "execute a command, passing the rest of the arguments",
        |mut app: App, args: &[&str]| {
            if args.len() < 1 {
                app.message(Message::Warning("expand needs a command and an argument string".to_string()));
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
                COMMAND_MAP.get(args[0])
                    .map(|sc| Completer::Of(sc, 1))
                    .unwrap_or(Completer::None)
            }),
        ])
    ),
    expand = (
        "execute a command, expanding the second argument into its arguments",
        |mut app: App, args: &[&str]| {
            if args.len() < 2 {
                app.message(Message::Warning("expand needs a command and an argument string".to_string()));
                return app;
            }
            if let Some(com) = COMMAND_MAP.get(args[0]) {
                let action = com.action;
                let args = split_line_args(&args[1], |name| app.lookup(&name));
                action(app, &args.iter().map(String::as_ref).collect::<Vec<_>>())
            } else {
                app.message(Message::Warning(format!("unknown command: '{}'", args[0])));
                app
            }
        },
        Completer::StaticWords(COMMAND_LIST)
    ),
    shell = (
        "execute a shell command, passing the rest as arguments",
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
                        app.message(Message::Error(format!("{}", err)));
                    }
                },
                _ => (), //app.message(Message::Error("no command given".to_string())),
            }
            app
        },
        Completer::StaticNth(&[Completer::PathLookup, Completer::None])
    ),
    prompt = (
        "prompt for input, then execute a command with it (the first argument should be the prompt text)",
        |mut app: App, args: &[&str]| {
            if args.len() < 2 {
                app.message(Message::Warning("prompt needs a prompt text, a command name and optional arguments".to_string()));
                return app;
            }
            if let Some(then) = COMMAND_MAP.get(args[1]) {
                app.prompt(args[0].to_string(), Action::Bind(then, args[2..].iter().map(|s| s.to_string()).collect()));
            } else {
                app.message(Message::Warning(format!("unknown command '{}'", args[0])));
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
        ])
    ),
    quit = (
        "save the state and quit with successful exit status",
        |mut app: App, _| {
            app.finish();
            app
        },
        Completer::None
    ),
    split = (
        "create a new split base on this view, horizontal or vertical",
        |mut app: App, args: &[&str]| {
            match args.get(0) {
                Some(&"horizontal") => {
                    app.view_split(Direction::Horizontal);
                    app
                }
                Some(&"vertical") => {
                    app.view_split(Direction::Vertical);
                    app
                }
                _ => app,
            }
        },
        Completer::StaticWords(&["horizontal", "vertical"])
    ),
    close_split = (
        "close the current focused view",
        |mut app: App, _| {
            app.view_close();
            app
        },
        Completer::None
    ),
    close_other_splits = (
        "close every other views, keeping only the current one",
        |mut app: App, _| {
            app.view_close_other();
            app
        },
        Completer::None
    ),
    transpose_splits = (
        "transpose the split containing the focused view",
        |mut app: App, _| {
            app.view_transpose();
            app
        },
        Completer::None
    ),
    fold_node = (
        "fold the node at the cursor",
        |mut app: App, _| {
            app.focused_mut().fold();
            app
        },
        Completer::None
    ),
    unfold_node = (
        "try to unfold the node at the cursor (nothing happens if it is not a directory or a link to one)",
        |mut app: App, _| {
            let (view, tree) = app.focused_and_tree_mut();
            match view.unfold(tree) {
                _ => (),
            }
            app
        },
        Completer::None
    ),
    enter_node = (
        "try to enter the node at the cursor, unfolding it first",
        |mut app: App, _| {
            let (view, tree) = app.focused_and_tree_mut();
            match view.unfold(tree) {
                Ok(()) => view.enter(),
                Err(_) => (),
            }
            app
        },
        Completer::None
    ),
    leave_node = (
        "leave the node at the cursor",
        |mut app: App, _| {
            app.focused_mut().leave();
            app
        },
        Completer::None
    ),
    next_node = (
        "next the node at the cursor",
        |mut app: App, _| {
            app.focused_mut().next();
            app
        },
        Completer::None
    ),
    prev_node = (
        "prev the node at the cursor",
        |mut app: App, _| {
            app.focused_mut().prev();
            app
        },
        Completer::None
    ),
    toggle_marked = (
        "mark the node at the cursor",
        |mut app: App, _| {
            app.focused_mut().toggle_marked();
            app
        },
        Completer::None
    ),
    scroll_view = (
        "scroll the focused view (vertically)",
        |mut app: App, args: &[&str]| {
            let by = args.get(0).map_or(Ok(1), |n| n.parse()).unwrap_or(1);
            app.focused_mut().view_offset().scroll += by;
            app
        },
        Completer::None
    ),
    shift_view = (
        "shift the focused view (horizontally)",
        |mut app: App, args: &[&str]| {
            let by = args.get(0).map_or(Ok(1), |n| n.parse()).unwrap_or(1);
            app.focused_mut().view_offset().shift += by;
            app
        },
        Completer::None
    ),
    to_view = (
        "move to a view right/left/down/up from the focused one",
        |mut app: App, args: &[&str]| {
            match args.get(0) {
                Some(&"right") => app.to_view(Direction::Horizontal, -1),
                Some(&"left") => app.to_view(Direction::Horizontal, 1),
                Some(&"down") => app.to_view(Direction::Vertical, 1),
                Some(&"up") => app.to_view(Direction::Vertical, -1),
                _ => (),
            }
            app
        },
        Completer::StaticWords(&["right", "left", "down", "up"])
    ),
    to_view_next = (
        "move to the next view in the same split",
        |mut app: App, _| {
            app.to_view_adjacent(1);
            app
        },
        Completer::None
    ),
    to_view_prev = (
        "move to the previous view in the same split",
        |mut app: App, _| {
            app.to_view_adjacent(-1);
            app
        },
        Completer::None
    ),
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
                    if rest.starts_with('{') {
                        if let Some((name, rest)) = rest[1..].split_once('}') {
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
        Completer::None
    ),
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
        Completer::None
    ),
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
        Completer::None
    ),
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
                .map(|s| s.chars().next().unwrap())
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
                COMMAND_MAP.get(args[1])
                    .map(|sc| Completer::Of(sc, 2))
                    .unwrap_or(Completer::None)
            }),
        ])
    ),
    bindings = (
        "list the current bindings",
        |mut app: App, _| {
            app.message(Message::Info({
                let b = app.get_bindings();
                format!("{b:#?}")
            }));
            app
        },
        Completer::None
    ),
    source = (
        "source the given file, executing each line as a command",
        |mut app: App, args: &[&str]| {
            let Some(res) = args.get(0).map(fs::read_to_string) else {
                app.message(Message::Warning(if args.is_empty() {
                    format!("could not read file '{}'", args[0])
                } else {
                    format!("source needs a file path")
                }));
                return app;
            };
            match res {
                Err(err) => app.message(Message::Warning(format!("could not read file: {err}"))),
                Ok(content) => {
                    for (k, com0_args) in content
                        .lines()
                        .map(|l| split_line_args(l, |_| String::new())) // interpolation in not enabled when sourcing
                        .enumerate()
                        .filter(|(_, v)| !v.is_empty())
                    {
                        let (com0, args) = com0_args.split_at(1);
                        let Some(act) = COMMAND_MAP.get(com0[0].as_str()).map(|c| c.action) else {
                            app.message(Message::Warning(format!("unknown command at line {k}")));
                            return app;
                        };
                        app = act(app, &args.iter().map(String::as_str).collect::<Vec<_>>());
                    }
                } // Ok
            } // match
            app
        },
        Completer::None
    ),
    sort = (
        "change the way nodes are sorted for the focused view",
        |mut app: App, args: &[&str]| {
            let prop = match args.get(0) {
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
                        skip += 1;
                        true
                    }
                    _ => false,
                },
            );
            view.renew_root(tree).unwrap();
            if let Some(rest) = args.get(skip) {
                app.message(Message::Warning(format!(
                    "unknown extraneous argument '{rest}'"
                )));
            }
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
                "ctime"
            ]),
            Completer::StaticWords(&["dirs_first", "reverse"]),
            Completer::StaticWords(&["reverse"]),
        ])
    ),
    keys = (
        "send key events as if theses where pressed by the user (whatever you are doing, this should really be last resort)",
        |app: App, args: &[&str]| {
            args.iter().fold(app, |app, ks| {
                ks.chars().fold(app, |app, k| {
                    app.do_event(&Event::Key(KeyEvent::new(
                        KeyCode::Char(k),
                        KeyModifiers::empty(),
                    )))
                })
            })
        },
        Completer::None
    ),
    message = (
        "display a message, level should be one of info/warning/error",
        |mut app: App, args: &[&str]| {
            match args.get(0) {
                Some(&"info") => app.message(Message::Info(args[1..].join(" "))),
                Some(&"warning") => app.message(Message::Warning(args[1..].join(" "))),
                Some(&"error") => app.message(Message::Error(args[1..].join(" "))),
                _ => app.message(Message::Warning("message needs a level and messages".to_string())),
            }
            app
        },
        Completer::StaticWords(&["info", "warning", "error"])
    ),
);
