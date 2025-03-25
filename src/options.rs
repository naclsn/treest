use std::env;
use std::fs::File;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process;

use anyhow::Result;
use thiserror::Error;

use crate::lua::help;
use crate::navigate::Navigate;
use crate::providers;

#[derive(Error, Debug)]
pub enum OptionsError {
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
    pub user_script: Option<PathBuf>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            provider_arg: ".".into(),
            provider_name: "fs".into(),
            user_script: dirs::config_dir()
                .map(|mut p| {
                    p.push("treest.lua");
                    p
                })
                .filter(|p| p.is_file()),
        }
    }
}

impl Options {
    pub fn parse_env() -> Result<Self, OptionsError> {
        Self::parse(env::args())
    }

    pub fn parse(mut args: impl Iterator<Item = String>) -> Result<Self, OptionsError> {
        let prog = args.next().unwrap();
        let mut r = Options::default();

        let mut pos_count = 0;
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--help" | "-h" => {
                    println!(
                        r#"Usage: {prog} [arg [name]]

    Navigate a tree-like space dynamically.

        --help | -h
        --list | -l
        --user | -u <in.lua>
        --lua-meta <out.lua>
        --defaults <out.lua>

    `arg` is passed to the provider `name`; if `name` is not given
    it's guessed from `arg`. See '--list' for a list of providers.
    Note: if `arg` is not given, it defaults to ".", so "fs" name.
    Use '--user' to provide a user config script sourced at start.

    Scripting is done through Lua 5.4. '--lua-meta' can generate a
    file containing the exposed API. use '--defaults' to prints or
    write the default config script. The default config can be use
    already in user config with `require('defaults')`.
"#
                    );
                    process::exit(2);
                }

                "--list" | "-l" => {
                    for name in providers::NAMES {
                        println!("{name}");
                    }
                    process::exit(3);
                }

                "--user" | "-u" => {
                    let user = args.next().ok_or(OptionsError::UserArgMissing)?;
                    let file = PathBuf::from(&user);
                    if !file.is_file() {
                        return Err(OptionsError::UserNotFile(user));
                    }
                    r.user_script = Some(file);
                }

                "--lua-meta" => {
                    let meta = args.next().unwrap_or("-".into());
                    if "-" == meta {
                        help::gen_lua_meta(&mut io::stdout()).unwrap();
                    } else {
                        help::gen_lua_meta(&mut File::create(meta).unwrap()).unwrap();
                    }
                    process::exit(4);
                }

                "--defaults" => {
                    let file = args.next().unwrap_or("-".into());
                    let defaults = include_str!("defaults.lua");
                    if "-" == file {
                        write!(&mut io::stdout(), "{}", defaults).unwrap();
                    } else {
                        write!(&mut File::create(file).unwrap(), "{}", defaults).unwrap();
                    }
                    process::exit(5);
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
