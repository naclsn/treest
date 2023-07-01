use crate::commands::StaticCommand;
use dirs::home_dir;
use std::{env, fmt, fs::Metadata, path::Path, path::PathBuf};

#[cfg(unix)]
fn is_executable_like(path: &Path, meta: &Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    if meta.is_symlink() {
        if let Ok(meta) = path.metadata() {
            meta.permissions()
        } else {
            meta.permissions()
        }
    } else {
        meta.permissions()
    }
    .mode()
        & 0o111
        != 0
}

#[cfg(windows)]
fn is_executable_like(path: &Path, _meta: &Metadata) -> bool {
    path.ends_with(".exe") || path.ends_with(".com")
}

#[derive(Clone)]
#[allow(dead_code)]
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
    FileFromRoot,
    FileFromCursor,
}

// XXX: remove, unusable
impl fmt::Display for Completer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let depth = f.precision().unwrap_or(0);
        let indent = "   ".repeat(depth);

        match self {
            Completer::None => write!(f, "{indent}nothing or anything"),

            Completer::Defered(_g) => write!(f, "{indent}arguments for a command"),

            Completer::Fn(func) => write!(f, "{indent}one of: {}", func(&[""], 0, 0).join(" ")),

            Completer::Words(v) => write!(f, "{indent}one of: {}", v.join(" ")),

            Completer::StaticWords(l) => write!(f, "{indent}one of: {}", l.join(" ")),

            Completer::Of(sc, _shift) => {
                write!(f, "{indent}{:.*}", depth + 1, sc.get_comp_itself())
            }

            Completer::Nth(v) => {
                writeln!(f, "{indent}positional arguments:")?;
                for it in v {
                    write!(f, "{it:.*}", depth + 1)?;
                    writeln!(f)?;
                }
                Ok(())
            }

            Completer::StaticNth(l) => {
                writeln!(f, "{indent}positional arguments:")?;
                for it in *l {
                    write!(f, "{it:.*}", depth + 1)?;
                    writeln!(f)?;
                }
                Ok(())
            }

            Completer::PathLookup => write!(f, "{indent}executable program from PATH"),

            Completer::FileFromRoot => write!(f, "{indent}file path from the root"),

            Completer::FileFromCursor => write!(f, "{indent}file path from the cursor"),
        }
    }
}

impl Completer {
    pub fn get_comp(
        &self,
        args: &[&str],
        arg_idx: usize,
        ch_idx: usize,
        lookup: &impl Fn(&str) -> Vec<String>,
    ) -> Vec<String> {
        match self {
            Completer::None => Vec::new(),

            Completer::Defered(g) => {
                g(args, arg_idx, ch_idx).get_comp(args, arg_idx, ch_idx, lookup)
            }

            Completer::Fn(func) => func(args, arg_idx, ch_idx),

            Completer::Words(v) => {
                let word = args[arg_idx];
                let (k, _) = word.char_indices().nth(ch_idx).unwrap_or((word.len(), ' '));
                let wor = &word[..k];
                let mut r: Vec<_> = v
                    .iter()
                    .filter(|it| it.starts_with(wor))
                    .map(|it| it.to_string() + " ")
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
                    .map(|it| it.to_string() + " ")
                    .collect();
                r.sort_unstable();
                r
            }

            Completer::Of(sc, shift) => {
                sc.get_comp(&args[*shift..], arg_idx - shift, ch_idx, lookup)
            }

            Completer::Nth(v) => v
                .get(arg_idx)
                .or(v.last())
                .map(|it| it.get_comp(args, arg_idx, ch_idx, lookup))
                .unwrap_or(Vec::new()),

            Completer::StaticNth(l) => l
                .get(arg_idx)
                .or(l.last())
                .map(|it| it.get_comp(args, arg_idx, ch_idx, lookup))
                .unwrap_or(Vec::new()),

            Completer::PathLookup => {
                let word = args[arg_idx];
                if word.starts_with('/') {
                    complete_file_from(args, arg_idx, ch_idx, "/", true)
                } else if word.contains('/') {
                    complete_file_from(args, arg_idx, ch_idx, &lookup("root")[0], true)
                } else {
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
                                                        if !meta.is_dir()
                                                            && is_executable_like(
                                                                &ent.path(),
                                                                &meta,
                                                            )
                                                        {
                                                            Some(name + " ")
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

            Completer::FileFromRoot => {
                complete_file_from(args, arg_idx, ch_idx, &lookup("root")[0], false)
            }

            Completer::FileFromCursor => {
                complete_file_from(args, arg_idx, ch_idx, &lookup("")[0], false)
            } // at cursor full path
        }
    }
}

fn complete_file_from(
    args: &[&str],
    arg_idx: usize,
    ch_idx: usize,
    path_from: &str,
    filter_exec: bool,
) -> Vec<String> {
    let word = args[arg_idx];
    let (k, _) = word.char_indices().nth(ch_idx).unwrap_or((word.len(), ' '));
    let wor = &word[..k];

    let partial_path = if let Some(stripped) = wor.strip_prefix("~/") {
        home_dir().unwrap().join(PathBuf::from(stripped))
    } else {
        let pb_wor = PathBuf::from(wor);
        if pb_wor.has_root() {
            pb_wor
        } else {
            PathBuf::from(path_from).join(pb_wor)
        }
    };

    let (search_in, search_for, add_slash) = if partial_path.is_dir() {
        // names in this directory
        (partial_path.as_path(), "", !wor.ends_with('/'))
    } else {
        // names in parent that starts with
        (
            partial_path.parent().unwrap(),
            partial_path.file_name().unwrap().to_str().unwrap(),
            false,
        )
    };

    search_in
        .read_dir()
        .ok()
        .map(|ent_iter| {
            let mut r: Vec<_> = ent_iter
                .filter_map(|maybe_ent| {
                    maybe_ent.ok().and_then(|ent| {
                        let path = ent.path();
                        let name = ent.file_name().to_string_lossy().to_string();
                        if let Some(stripped) = name.strip_prefix(search_for) {
                            match ent.metadata() {
                                Ok(meta) if !filter_exec || is_executable_like(&path, &meta) => {
                                    let mut opt = stripped.to_string();
                                    opt.push(if meta.is_dir() { '/' } else { ' ' });
                                    if add_slash {
                                        opt.insert(0, '/');
                                    }
                                    opt.insert_str(0, wor);
                                    Some(opt)
                                }
                                _ => None,
                            }
                        } else {
                            None
                        }
                    }) // maybe_ent |ent|
                }) // ent_iter |maybe_ent|
                .collect::<Vec<String>>();
            r.sort_unstable();
            r.dedup();
            r
        }) // dir |ent_iter|
        .unwrap_or(Vec::new())
}
