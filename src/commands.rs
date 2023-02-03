use crate::app::App;
use lazy_static::lazy_static;
use std::{collections::HashMap, default::Default};
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
                    ('h', to_view_right),
                    ('l', to_view_left),
                    ('j', to_view_down),
                    ('k', to_view_up),
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
                        app.focused_mut().cursor.clear();
                        app
                    })
                }),]
            ),
            ('y', (scroll_view, "-3")),
            ('e', (scroll_view, "3")),
            ('Y', (shift_view, "-3")),
            ('E', (shift_view, "3")),
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
        fn $name(app: App, args: &[&str]) -> App {
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
    command = ("command", |app: App, args: &[&str]| {
        if let Some(com) = COMMAND_MAP.get(args[0]) {
            let action = com.action;
            action(app, &args[1..])
        } else {
            app
        }
    }),
    quit = ("quit", |mut app: App, _| {
        app.finish();
        app
    }),
    split = ("split", |app: App, args: &[&str]| {
        match args.get(0) {
            Some(&"horizontal") => app.view_split(Direction::Horizontal),
            Some(&"vertical") => app.view_split(Direction::Vertical),
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
    to_view_right = ("to_view_right", |mut app: App, _| {
        app.to_view_right();
        app
    }),
    to_view_left = ("to_view_left", |mut app: App, _| {
        app.to_view_left();
        app
    }),
    to_view_down = ("to_view_down", |mut app: App, _| {
        app.to_view_down();
        app
    }),
    to_view_up = ("to_view_up", |mut app: App, _| {
        app.to_view_up();
        app
    }),
);
