mod app;
mod args;
mod commands;
mod completions;
mod line;
mod node;
mod textblock;
mod tree;
mod view;

use crate::{
    app::{App, AppState},
    args::Args,
    commands::Action,
};
use clap::Parser;
use crossterm::{
    event::{self, Event as IOEvent}, //::{self, DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use dirs::home_dir;
use notify::{recommended_watcher, Event as FSEvent, RecursiveMode, Watcher};
use std::{
    env::{current_dir, set_current_dir},
    error::Error,
    fs, io,
    path::{Component, Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
    thread,
};
use tui::{backend::CrosstermBackend, terminal::Terminal};

struct TerminalWrap<W: io::Write>(Terminal<CrosstermBackend<W>>);

impl<W: io::Write> TerminalWrap<W> {
    fn new(mut bla: W) -> Result<TerminalWrap<W>, Box<dyn Error>> {
        enable_raw_mode()?;
        execute!(bla, EnterAlternateScreen /*, EnableMouseCapture*/)?;
        let backend = CrosstermBackend::new(bla);
        let terminal = Terminal::new(backend)?;
        Ok(TerminalWrap(terminal))
    }

    fn regrab(&mut self) -> Result<(), Box<dyn Error>> {
        enable_raw_mode()?;
        execute!(
            self.0.backend_mut(),
            EnterAlternateScreen,
            // EnableMouseCapture
        )?;
        self.0.hide_cursor()?;
        Ok(())
    }

    fn release(&mut self) -> Result<(), Box<dyn Error>> {
        disable_raw_mode()?;
        execute!(
            self.0.backend_mut(),
            LeaveAlternateScreen,
            // DisableMouseCapture
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
    acc.push("treest");
    acc
}

#[derive(Debug)]
enum ExternalEvent {
    None,
    IOEvent(IOEvent),
    FSEvent(FSEvent),
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

    if args.changedir {
        set_current_dir(&dir)?;
    }

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
            app = Action::Fn(&commands::cmd::source).apply(app, &[&p.to_string_lossy()]);
        }
    }

    let mut terminal = TerminalWrap::new(io::stderr())?;
    // draw once as soon as possible
    terminal.0.draw(|f| app.draw(f))?;

    let ex_event = Arc::new((Mutex::new(ExternalEvent::None), Condvar::new()));

    // setup events: IO (user inputs) and FS (files add/rm)
    let ex_event_io_clone = Arc::clone(&ex_event); // moved
    thread::spawn(move || {
        while let Ok(io_ev) = event::read() {
            let (lock, cvar) = &*ex_event_io_clone;
            let mut ev = lock.lock().unwrap();
            *ev = ExternalEvent::IOEvent(io_ev);
            cvar.notify_one();
        }
    });

    let ex_event_fs_clone = Arc::clone(&ex_event); // moved
    let mut watcher = recommended_watcher(move |res| {
        if let Ok(fs_ev) = res {
            let (lock, cvar) = &*ex_event_fs_clone;
            let mut ev = lock.lock().unwrap();
            *ev = ExternalEvent::FSEvent(fs_ev);
            cvar.notify_one();
        }
    })?;
    // TODO: pass `watcher` to `app` and such, watch unfolded
    // directory non-recursively (carefull of recursive symlinks!)
    watcher.watch(Path::new("."), RecursiveMode::NonRecursive)?;

    let (lock, cvar) = &*ex_event;
    let mut event = lock.lock().unwrap();

    loop {
        app = match &*event {
            ExternalEvent::None => app,
            ExternalEvent::IOEvent(io_ev) => app.do_event(io_ev),
            ExternalEvent::FSEvent(_fs_ev) => Action::Fn(&commands::cmd::reload).apply(app, &[]),
        };
        *event = ExternalEvent::None;

        match app.state {
            AppState::None => (),
            AppState::Quit => break,
            AppState::Pause => {
                #[cfg(not(windows))]
                {
                    terminal.release()?;
                    signal_hook::low_level::raise(signal_hook::consts::signal::SIGTSTP)?;
                    terminal.regrab()?;
                    terminal.0.clear()?;
                }
                app.state = AppState::None;
            }
            AppState::Pending(does) => {
                terminal.release()?;
                app.state = AppState::None;
                app = does(app);
                terminal.regrab()?;
                terminal.0.clear()?;
            }
            AppState::Sourcing(_) => unreachable!(),
        }

        terminal.0.draw(|f| app.draw(f))?;
        event = cvar.wait(event).unwrap();
    }

    // TODO: get each views' root paths and save individually there
    //       so that is plays better with reroot
    if let Some(parent) = save_at.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(&save_at, serde_json::to_string(&app)?)?;
    Ok(())
}
