use crate::app::App;
use lazy_static::lazy_static;
use std::{collections::HashMap, default::Default};

type Action = fn(App /*, &[&str]*/) -> App;

#[derive(Debug)]
enum Command {
    Immediate(Action),
    Pending(HashMap<char, Command>),
}

#[derive(Debug)]
pub struct CommandMap(HashMap<char, Command>);

macro_rules! make_map_one {
    ($key:literal => $func:ident) => {
        ($key, Command::Immediate($func))
    };
    ($key:literal => [$(($key2:literal, $value:tt),)*]) => {
        ($key, Command::Pending(make_map!($(($key2, $value),)*)))
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
        CommandMap(make_map!(
            ('q', quit),
            ('Q', quit),
            ('w', [('s', split_horizontal), ('v', split_vertical),]),
            ('?', help),
            (':', command),
            ('h', leave_node),
            ('l', enter_node),
            ('j', next_node),
            ('k', prev_node),
        ))
    }
}

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

#[derive(Debug)]
pub struct StaticCommand {
    name: &'static str,
    doc: &'static str,
    action: Action,
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
        fn $name(app: App) -> App {
            $action(app)
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
    help = ("help", |app: App| {
        for (_, com) in COMMAND_MAP.iter() {
            println!("{}:\n\t{}", com.name, com.doc);
        }
        app
    }),
    command = ("command", |app: App| {
        if let Some(com) = COMMAND_MAP.get("help") {
            let action = com.action;
            action(app)
        } else {
            app
        }
    }),
    quit = ("quit", |mut app: App| {
        app.finish();
        app
    }),
    split_horizontal = ("split_horizontal", App::split_horizontal),
    split_vertical = ("split_vertical", App::split_vertical),
    enter_node = ("enter_node", |mut app: App| {
        let (view, tree) = app.focused_and_tree_mut();
        match view.unfold(tree) {
            Ok(()) => view.enter(),
            Err(_) => (),
        }
        app
    }),
    leave_node = ("leave_node", |mut app: App| {
        app.focused_mut().leave();
        app
    }),
    next_node = ("next_node", |mut app: App| {
        app.focused_mut().next();
        app
    }),
    prev_node = ("prev_node", |mut app: App| {
        app.focused_mut().prev();
        app
    }),
);
