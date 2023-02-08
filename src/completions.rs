use crate::commands::COMMAND_LIST;

#[derive(Clone)]
pub enum Completer {
    Fn(fn(&[&str], usize, usize) -> Vec<String>),
    Words(Vec<String>),
    StaticWords(&'static [&'static str]),
}

impl Completer {
    pub fn get_comp(&self, args: &[&str], arg_idx: usize, ch_idx: usize) -> Vec<String> {
        match self {
            Completer::Fn(func) => func(args, arg_idx, ch_idx),

            Completer::Words(v) => {
                let word = args[arg_idx];
                let (k, _) = word.char_indices().nth(ch_idx).unwrap_or((word.len(), ' '));
                let wor = &word[..k];
                let mut r: Vec<_> = v
                    .iter()
                    .filter(|it| it.starts_with(wor))
                    .map(String::clone)
                    .collect();
                r.sort_unstable();
                r
            }

            Completer::StaticWords(l) => {
                let word = args[arg_idx];
                let (k, _) = word.char_indices().nth(ch_idx).unwrap_or((word.len(), ' '));
                let wor = &word[..k];
                let mut r: Vec<_> = l
                    .iter()
                    .filter(|it| it.starts_with(wor))
                    .map(|it| it.to_string())
                    .collect();
                r.sort_unstable();
                r
            }
        }
    }
}

pub const comp_none: Completer = Completer::Fn(|_, _, _| Vec::new());

pub const fn comp_commands() -> Completer {
    Completer::StaticWords(COMMAND_LIST)
}
