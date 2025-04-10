use std::env;
use std::fs::File;
use std::io::{self, IsTerminal, Write};
use std::os::fd::AsRawFd;
use std::path::PathBuf;
use std::process;

use anyhow::Result;
use thiserror::Error;

use crate::lua::help;
use crate::navigate::Navigate;
use crate::provider;

#[derive(Error, Debug)]
pub enum OptionsError {
    #[error("could not get a terminal file descriptor")]
    NoTerminal,
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
                    for name in provider::NAMES {
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
                    let defaults = crate::include_etc!("defaults.lua");
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
                    if !provider::NAMES.contains(&name) {
                        return Err(OptionsError::NotProvider(arg));
                    }
                    pos_count += 1;
                    r.provider_name = arg;
                }

                _ => return Err(OptionsError::UnexpectedArg(arg)),
            }
        }

        if 1 == pos_count {
            let Some(name) = provider::guess(&r.provider_arg) else {
                return Err(OptionsError::ProviderNeeded);
            };
            r.provider_name = name.into();
        }

        let in_tty = io::stdin().is_terminal();
        let err_tty = io::stderr().is_terminal();
        if !in_tty || !err_tty {
            #[cfg(windows)]
            return Err(OptionsError::NoTerminal);
            #[cfg(not(windows))]
            match File::open("/dev/tty") {
                Ok(f) => unsafe {
                    if libc::dup2(f.as_raw_fd(), libc::STDIN_FILENO) < 0 {
                        return Err(OptionsError::NoTerminal);
                    }
                    if libc::dup2(f.as_raw_fd(), libc::STDERR_FILENO) < 0 {
                        return Err(OptionsError::NoTerminal);
                    }
                },
                Err(_) => return Err(OptionsError::NoTerminal),
            }
        }

        Ok(r)
    }

    pub fn instanciate(self) -> Result<Navigate> {
        let mut nav = Navigate::new(self.user_script);
        {
            let provider = provider::select(&self.provider_arg, &self.provider_name)?;
            nav.insert_space(1, provider, self.provider_name);
            nav.remove_space(0);
        }
        Ok(nav)
    }
}
