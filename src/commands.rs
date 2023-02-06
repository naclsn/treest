use crate::{
    app::App,
    line::{split_line_args, Message},
};
use lazy_static::lazy_static;
use std::{collections::HashMap, default::Default, fs, io, process::Command as SysCommand};
use tui::layout::Direction;

#[derive(Clone)]
pub enum Action {
    Fn(fn(App, &[&str]) -> App),
    Bind(fn(App, &[&str]) -> App, Vec<String>),
    Chain(Vec<Action>),
}

impl Action {
    pub fn apply(&self, app: App, args: &[&str]) -> App {
        match self {
            Action::Fn(func) => func(app, args),
            Action::Bind(func, bound) => {
                let mut v: Vec<_> = bound.iter().map(String::as_str).collect();
                v.extend_from_slice(args);
                func(app, &v)
            }
            Action::Chain(funcs) => funcs.iter().fold(app, |acc, cur| cur.apply(acc, args)),
        }
    }
}

#[derive(Clone)]
enum Command {
    Immediate(Action),
    Pending(HashMap<char, Command>),
}

#[derive(Clone)]
pub struct CommandMap(HashMap<char, Command>);

macro_rules! make_map_one {
    ($key:literal => [$(($key2:literal, $value:tt),)*]) => {
        ($key, Command::Pending(make_map!($(($key2, $value),)*)))
    };
    ($key:literal => $action:tt) => {
        ($key, Command::Immediate(make_map_one!(@ $action)))
    };
    (@ $func:ident) => {
        Action::Fn($func)
    };
    (@ ($func:ident, $($bound:literal),+)) => {
        Action::Bind($func, vec![$($bound.to_string()),+])
    };
    (@ ($($actions:tt),*)) => {
        Action::Chain(vec![$(make_map_one!(@ $actions)),*])
    };
    (@ $action:expr) => {
        $action
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
                    Action::Fn(|mut app: App, _| {
                        app.focused_mut().cursor_to_root();
                        app
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
                Some(Command::Immediate(action)) => return (Some(action), false),
                Some(Command::Pending(next)) => curr = next.get(key),
            }
        }

        if let Some(Command::Immediate(action)) = curr {
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
                    Some(Command::Pending(_)) => {
                        let Some(Command::Pending(next)) = acc.get_mut(ch) else { unreachable!(); };
                        next
                    }
                    Some(Command::Immediate(_)) | None => {
                        let niw = HashMap::new();
                        acc.insert(*ch, Command::Pending(niw));
                        let Some(Command::Pending(niw)) = acc.get_mut(ch) else { unreachable!(); };
                        niw
                    }
                }
            };
        }
        acc.insert(key_path[key_path.len() - 1], Command::Immediate(action));
    }
}

pub struct StaticCommand {
    name: &'static str,
    doc: &'static str,
    action: fn(App, &[&str]) -> App,
}

macro_rules! make_lst_one {
    ($name:ident: $doc:literal) => {
        (
            stringify!($name),
            StaticCommand {
                name: stringify!($name),
                doc: $doc,
                action: $name,
            },
        )
    };
    ($name:ident = $action:expr) => {
        pub fn $name(app: App, args: &[&str]) -> App {
            $action(app, args)
        }
    };
}

macro_rules! make_lst {
    ($($name:ident = ($doc:literal, $action:expr),)*) => {
        $(make_lst_one!($name = $action);)*

        lazy_static! {
            static ref COMMAND_MAP: HashMap<&'static str, StaticCommand> =
                HashMap::from([$(make_lst_one!($name: $doc),)*]);
        }
    };
}

make_lst!(
    help = ("help", |app: App, _| {
        for (_, com) in COMMAND_MAP.iter() {
            println!("{}:\n\t{}", com.name, com.doc);
        }
        app
    }),
    command = ("command", |mut app: App, args: &[&str]| {
        if let Some(com) = COMMAND_MAP.get(args[0]) {
            let action = com.action;
            action(app, &args[1..])
        } else {
            app.message(Message::Warning(if args.is_empty() {
                format!("no command name provided")
            } else {
                format!("unknown command: '{}'", args[0])
            }));
            app
        }
    }),
    shell = ("shell", |mut app: App, args: &[&str]| {
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
    }),
    prompt = ("prompt", |mut app: App, args: &[&str]| {
        match (args.get(0), args.get(1).and_then(|n| COMMAND_MAP.get(n))) {
            (Some(prompt), Some(then)) => app.prompt(prompt.to_string(), Action::Fn(then.action)),
            _ => (),
        }
        app
    }),
    quit = ("quit", |mut app: App, _| {
        app.finish();
        app
    }),
    split = ("split", |mut app: App, args: &[&str]| {
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
    }),
    close_split = ("close_split", |mut app: App, _| {
        app.view_close();
        app
    }),
    transpose_splits = ("transpose_splits", |mut app: App, _| {
        app.view_transpose();
        app
    }),
    fold_node = ("fold_node", |mut app: App, _| {
        app.focused_mut().fold();
        app
    }),
    unfold_node = ("unfold_node", |mut app: App, _| {
        let (view, tree) = app.focused_and_tree_mut();
        match view.unfold(tree) {
            _ => (),
        }
        app
    }),
    enter_node = ("enter_node", |mut app: App, _| {
        let (view, tree) = app.focused_and_tree_mut();
        match view.unfold(tree) {
            Ok(()) => view.enter(),
            Err(_) => (),
        }
        app
    }),
    leave_node = ("leave_node", |mut app: App, _| {
        app.focused_mut().leave();
        app
    }),
    next_node = ("next_node", |mut app: App, _| {
        app.focused_mut().next();
        app
    }),
    prev_node = ("prev_node", |mut app: App, _| {
        app.focused_mut().prev();
        app
    }),
    toggle_marked = ("toggle_marked", |mut app: App, _| {
        app.focused_mut().toggle_marked();
        app
    }),
    scroll_view = ("scroll_view", |mut app: App, args: &[&str]| {
        let by = args.get(0).map_or(Ok(1), |n| n.parse()).unwrap_or(1);
        app.focused_mut().offset.scroll += by;
        app
    }),
    shift_view = ("shift_view", |mut app: App, args: &[&str]| {
        let by = args.get(0).map_or(Ok(1), |n| n.parse()).unwrap_or(1);
        app.focused_mut().offset.shift += by;
        app
    }),
    to_view = ("to_view", |mut app: App, args: &[&str]| {
        match args.get(0) {
            Some(&"right") => app.to_view(Direction::Horizontal, -1),
            Some(&"left") => app.to_view(Direction::Horizontal, 1),
            Some(&"down") => app.to_view(Direction::Vertical, 1),
            Some(&"up") => app.to_view(Direction::Vertical, -1),
            _ => (),
        }
        app
    }),
    to_view_next = ("to_view_next", |mut app: App, _| {
        app.to_view_adjacent(1);
        app
    }),
    to_view_prev = ("to_view_prev", |mut app: App, _| {
        app.to_view_adjacent(-1);
        app
    }),
    echo = ("echo", |app: App, args: &[&str]| {
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
    }),
    declare = ("declare", |mut app: App, args: &[&str]| {
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
    }),
    read = ("read", |mut app: App, args: &[&str]| {
        let mut line = String::new();
        let Ok(_) = io::stdin().read_line(&mut line) else { return app; };
        let mut values = line.split_whitespace();
        for it in args {
            app.declare(it, values.next().unwrap_or(""));
        }
        app
    }),
    bind = ("bind", |mut app: App, args: &[&str]| {
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
        let Some(action) = COMMAND_MAP.get(args[1]).map(|c| c.action) else {
            app.message(Message::Warning(format!("cannot bind unknown command '{}'", args[1])));
            return app;
        };
        let bound = &args[2..];
        app.rebind(
            &key_path,
            Action::Bind(action, bound.iter().map(|s| s.to_string()).collect()),
        );
        app
    }),
    source = ("source", |mut app: App, args: &[&str]| {
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
                    .map(split_line_args)
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
    }),
);
