use crate::commands::StaticCommand;
use std::{env, fs::Metadata};

#[cfg(unix)]
fn is_executable_like(_name: &String, meta: &Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & 0o111 != 0
}

#[cfg(windows)]
fn is_executable_like(name: &String, _meta: &Metadata) -> bool {
    name.ends_with(".exe") || name.ends_with(".com")
}

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
    PathLookup,
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

            Completer::PathLookup => {
                let word = args[arg_idx];
                let (k, _) = word.char_indices().nth(ch_idx).unwrap_or((word.len(), ' '));
                let wor = &word[..k];
                env::var_os("PATH")
                    .map(|paths| {
                        let mut r = env::split_paths(&paths)
                            .filter_map(|dir| {
                                dir.read_dir().ok().map(|ent_iter| {
                                    ent_iter.filter_map(|maybe_ent| {
                                        maybe_ent.ok().and_then(|ent| {
                                            let name =
                                                ent.file_name().to_string_lossy().to_string();
                                            if name.starts_with(wor) {
                                                ent.metadata().ok().and_then(|meta| {
                                                    if meta.is_file()
                                                        && is_executable_like(&name, &meta)
                                                    {
                                                        Some(name)
                                                    } else {
                                                        None
                                                    }
                                                }) // ent |meta|
                                            } else {
                                                None
                                            }
                                        }) // maybe_ent |ent|
                                    }) // ent_iter |maybe_ent|
                                }) // dir |ent_iter|
                            })
                            .flatten()
                            .collect::<Vec<String>>();
                        r.sort_unstable();
                        r.dedup();
                        r
                    })
                    .unwrap_or(Vec::new())
            }
        }
    }
}
