use crate::commands::StaticCommand;

#[derive(Clone)]
pub enum Completer {
    None,
    Fn(fn(&[&str], usize, usize) -> Vec<String>),
    Defered(fn(&[&str], usize, usize) -> Completer),
    Words(Vec<String>),
    StaticWords(&'static [&'static str]),
    Of(&'static StaticCommand, usize),
    Nth(Vec<Completer>),
    StaticNth(&'static [Completer]),
}

impl Completer {
    pub fn get_comp(&self, args: &[&str], arg_idx: usize, ch_idx: usize) -> Vec<String> {
        match self {
            Completer::None => Vec::new(),

            Completer::Defered(g) => g(args, arg_idx, ch_idx).get_comp(args, arg_idx, ch_idx),

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

            Completer::Of(sc, shift) => sc.get_comp(&args[*shift..], arg_idx - shift, ch_idx),

            Completer::Nth(v) => v
                .get(arg_idx)
                .or(v.last())
                .map(|it| it.get_comp(args, arg_idx, ch_idx))
                .unwrap_or(Vec::new()),

            Completer::StaticNth(v) => v
                .get(arg_idx)
                .or(v.last())
                .map(|it| it.get_comp(args, arg_idx, ch_idx))
                .unwrap_or(Vec::new()),
        }
    }
}
