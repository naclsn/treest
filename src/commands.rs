use crate::app::App;
use std::{collections::HashMap, default::Default};

#[derive(Debug)]
enum Command {
    Immediate(fn(App) -> App),
    Pending(HashMap<char, Command>),
}

#[derive(Debug)]
pub struct CommandMap(HashMap<char, Command>);

impl Default for CommandMap {
    fn default() -> CommandMap {
        CommandMap(HashMap::from([
            (
                'q',
                Command::Immediate(|mut app| {
                    app.finish();
                    app
                }),
            ),
            (
                'Q',
                Command::Immediate(|mut app| {
                    app.finish();
                    app
                }),
            ),
            (
                'w',
                Command::Pending(HashMap::from([
                    ('s', Command::Immediate(App::split_horizontal)),
                    ('v', Command::Immediate(App::split_vertical)),
                ])),
            ),
        ]))
    }
}

impl CommandMap {
    pub fn try_get_action(&self, key_path: &[char]) -> (Option<&fn(App) -> App>, bool) {
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
