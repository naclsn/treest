mod app;
mod args;
mod commands;
mod completions;
mod line;
mod node;
mod textblock;
mod tree;
mod view;

use crate::{app::App, args::Args, commands::Action};
use clap::Parser;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode},
};
use dirs::home_dir;
use std::{
    env::current_dir,
    error::Error,
    fs,
    io::{self, Write},
    path::{Component, Path, PathBuf},
};
use tui::{
    backend::{Backend, CrosstermBackend},
    terminal::Terminal,
};

struct TerminalWrap<W: io::Write>(Terminal<CrosstermBackend<W>>);

impl<W: io::Write> TerminalWrap<W> {
    fn new(mut bla: W) -> Result<TerminalWrap<W>, Box<dyn Error>> {
        enable_raw_mode()?;
        execute!(
            bla,
            //EnterAlternateScreen,
            EnableMouseCapture
        )?;
        let backend = CrosstermBackend::new(bla);
        io::stderr().write_all(&vec![b'\n'; backend.size()?.height.into()])?;
        let terminal = Terminal::new(backend)?;
        Ok(TerminalWrap(terminal))
    }

    fn regrab(&mut self) -> Result<(), Box<dyn Error>> {
        enable_raw_mode()?;
        execute!(
            self.0.backend_mut(),
            //EnterAlternateScreen,
            EnableMouseCapture
        )?;
        self.0.hide_cursor()?;
        Ok(())
    }

    fn release(&mut self) -> Result<(), Box<dyn Error>> {
        disable_raw_mode()?;
        execute!(
            self.0.backend_mut(),
            //LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        self.0.show_cursor()?;
        Ok(())
    }
}

impl<W: io::Write> Drop for TerminalWrap<W> {
    fn drop(&mut self) {
        self.release().unwrap();
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let res = run_app(Args::parse());
    print!("\r\n");

    if let Err(err) = res {
        println!("{err:?}");
    }

    Ok(())
}

fn get_save_path(dir: &Path) -> PathBuf {
    let mut acc = home_dir().unwrap();
    acc.push(".cache");
    acc.push("treest");
    acc.push("root");
    for cur in dir.components() {
        if let Component::Normal(it) = cur {
            acc.push(it);
        }
    }
    acc.push("save.json");
    acc
}

fn get_default_userconf_path() -> PathBuf {
    let mut acc = home_dir().unwrap();
    acc.push(".config");
    acc.push("treestrc");
    acc
}

fn run_app(args: Args) -> Result<(), Box<dyn Error>> {
    let cwd = current_dir()?;

    let dir = {
        let r = args.path.unwrap_or(cwd.clone());
        if r.is_absolute() {
            r
        } else {
            cwd.join(r)
        }
    }
    .canonicalize()?;
    let save_at = get_save_path(&dir);

    let mut app = {
        if args.clearstate {
            App::new(dir)?
        } else if let Ok(content) = fs::read_to_string(&save_at) {
            let mut r: App = serde_json::from_str(&content)?;
            r.fixup();
            r
        } else {
            App::new(dir)?
        }
    };

    if !args.clean {
        let p = args.userconf.unwrap_or_else(get_default_userconf_path);
        if p.exists() {
            app = Action::Fn(&commands::source).apply(app, &[&p.to_string_lossy()]);
        }
    }

    let mut terminal = TerminalWrap::new(io::stderr())?;

    while !app.done() {
        terminal.0.draw(|f| app.draw(f))?;
        app = app.do_event(&event::read()?);

        if app.stopped() {
            #[cfg(not(windows))]
            {
                terminal.release()?;
                signal_hook::low_level::raise(signal_hook::consts::signal::SIGTSTP)?;
                terminal.regrab()?;
                terminal.0.clear()?;
            }
            app.resume();
        }
    }

    if let Some(parent) = save_at.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(&save_at, serde_json::to_string(&app)?)?;
    Ok(())
}
