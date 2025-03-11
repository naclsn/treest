use std::path::PathBuf;

use anyhow::Result;
use thiserror::Error;

use crate::navigate::Navigate;
use crate::providers;

#[derive(Error, Debug)]
pub enum OptionsError {
    #[error(
        r#"Usage: {0} [arg [name]]

    Navigate a tree-like space dynamically.

    `arg` is passed to the provider `name`; if `name` is not given
    it's guessed from `arg`. See '--list' for a list of providers.
    Note: if `arg` is not given, it defaults to ".", so "fs" name.
    Use '--user' to provide a user config script sourced at start.
"#
    )]
    Help(String),
    #[error("{0}")]
    List(String),
    #[error("unexpected extra argument '{0}'")]
    UnexpectedArg(String),
    #[error("the provider to use could not be guessed from the argument (see '--list')")]
    ProviderNeeded,
    #[error("'{0}' does not name an existing provider (see '--list')")]
    NotProvider(String),
    #[error("no file path given to --user")]
    UserArgMissing,
    #[error("could not read file '{0}' given to --user")]
    UserNotFile(String),
}

#[derive(Debug, Clone)]
pub struct Options {
    pub provider_arg: String,
    pub provider_name: String,
    pub user_script: Option<PathBuf>, // `take`n in main_loop
}

impl Default for Options {
    fn default() -> Self {
        Self {
            provider_arg: ".".into(),
            provider_name: "fs".into(),
            user_script: Some(PathBuf::from("treest.rhai")).filter(|p| p.is_file()),
        }
    }
}

impl Options {
    pub fn parse(mut args: impl Iterator<Item = String>) -> Result<Options, OptionsError> {
        let prog = args.next().unwrap();
        let mut r = Options::default();

        let mut pos_count = 0;
        while let Some(arg) = args.next() {
            match arg.as_str() {
                // yea why nat
                "=" => rhai_interp(),

                "--list" | "-l" => return Err(OptionsError::List(providers::NAMES.join("\n"))),

                "--help" | "-h" => return Err(OptionsError::Help(prog)),

                "--user" | "-u" => {
                    let user = args.next().ok_or(OptionsError::UserArgMissing)?;
                    let file = PathBuf::from(&user);
                    if !file.is_file() {
                        return Err(OptionsError::UserNotFile(user));
                    }
                    r.user_script = Some(file);
                }

                "-" if 0 == pos_count => {
                    pos_count += 1;
                    r.provider_arg.clear();
                }
                "--" if 0 == pos_count => {
                    pos_count += 1;
                    r.provider_arg = args.next().unwrap_or_default();
                }
                _ if 0 == pos_count => {
                    pos_count += 1;
                    r.provider_arg = arg;
                }
                name if 1 == pos_count => {
                    if !providers::NAMES.contains(&name) {
                        return Err(OptionsError::NotProvider(arg));
                    }
                    pos_count += 1;
                    r.provider_name = arg;
                }

                _ => return Err(OptionsError::UnexpectedArg(arg)),
            }
        }

        if 1 == pos_count {
            let Some(name) = providers::guess(&r.provider_arg) else {
                return Err(OptionsError::ProviderNeeded);
            };
            r.provider_name = name.into();
        }

        Ok(r)
    }

    pub fn instanciate(self) -> Result<Navigate> {
        providers::select(&self.provider_arg, &self.provider_name)
            .map(|prov| Navigate::new(self.user_script, prov, self.provider_name))
    }
}

fn rhai_interp() {
    use std::io::{self, IsTerminal, Read};
    use std::process;

    use rhai::{Dynamic, Scope};

    use crate::navigate;
    use crate::prompt;
    use crate::terminal;

    let mut input = io::stdin();
    let output = io::stderr();
    let mut history = Vec::new();
    let engine = navigate::make_engine();
    let mut scope = Scope::new();

    if input.is_terminal() {
        let mut restore = terminal::raw().unwrap();

        let mut input = input.bytes().map_while(Result::ok);
        while let Some(line) =
            prompt::prompt("?? ", &mut input, &output, history.clone(), |_, _| {
                Vec::new()
            })
        {
            restore.restore();
            eprintln!();

            match engine.compile(&line) {
                Ok(ast) => {
                    eprintln!("{ast:#?}");
                    match engine.eval_ast_with_scope::<Dynamic>(&mut scope, &ast) {
                        Ok(ans) => {
                            if !ans.is_unit() {
                                eprintln!(":: {ans:#?}");
                            }
                        }
                        Err(err) => eprintln!("!! {err}"),
                    }
                }
                Err(err) => eprintln!("!! {err}"),
            }
            history.push(line);

            restore = terminal::raw().unwrap();
        }

        restore.restore();
    } else {
        let mut s = String::new();
        if input.read_to_string(&mut s).is_ok() {
            match engine.eval::<Dynamic>(&s) {
                Ok(ans) => eprintln!("{ans:#?}"),
                Err(err) => eprintln!("!! {err}"),
            }
        }
    }

    process::exit(0);
}
