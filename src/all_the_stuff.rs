use crate::{
    app::{App, AppState},
    args::Args,
    commands::{cmd, Action},
};
use crossterm::{
    event::{self, Event as IOEvent}, //::{self, DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use dirs::home_dir;
use notify::{recommended_watcher, Event as FSEvent, RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    env::{current_dir, set_current_dir},
    error::Error,
    fs, io,
    path::{Component, Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
    thread,
};
use tui::{backend::CrosstermBackend, terminal::Terminal};

// TODO: clean things

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

#[derive(Debug)]
enum ExternalEvent {
    None,
    IOEvent(IOEvent),
    FSEvent(FSEvent),
}

pub struct AllTheStuff {
    app: App,
    terminal: TerminalWrap<io::Stderr>, // moved into app (possible?)
    watcher: RecommendedWatcher,        // moved into app
    ex_event: Arc<(Mutex<ExternalEvent>, Condvar)>, // moved into app
}

impl AllTheStuff {
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

    pub fn new(args: Args) -> Result<Self, Box<dyn Error>> {
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

        if args.changedir {
            set_current_dir(&dir)?;
        }

        let mut app = {
            if args.clearstate {
                App::new(dir)?
            } else if let Ok(content) = fs::read_to_string(Self::get_save_path(&dir)) {
                if let Ok(mut r) = serde_json::from_str::<App>(&content) {
                    r.fixup();
                    r
                } else {
                    App::new(dir)?
                }
            } else {
                App::new(dir)?
            }
        };

        if !args.clean {
            let p = args
                .userconf
                .unwrap_or_else(Self::get_default_userconf_path);
            if p.exists() {
                app = Action::Fn(&cmd::source).apply(app, &[&p.to_string_lossy()]);
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

        Ok(Self {
            app,
            terminal,
            watcher,
            ex_event,
        })
    } // fn new

    pub fn run(mut self) -> Result<(), Box<dyn Error>> {
        let (lock, cvar) = &*self.ex_event;
        let mut event = lock.lock().unwrap();

        loop {
            self.app = match &*event {
                ExternalEvent::None => self.app,
                ExternalEvent::IOEvent(io_ev) => self.app.do_event(io_ev),
                ExternalEvent::FSEvent(_fs_ev) => Action::Fn(&cmd::reload).apply(self.app, &[]),
            };
            *event = ExternalEvent::None;

            match self.app.state {
                AppState::None => (),
                AppState::Quit => break,
                AppState::Pause => {
                    #[cfg(not(windows))]
                    {
                        self.terminal.release()?;
                        signal_hook::low_level::raise(signal_hook::consts::signal::SIGTSTP)?;
                        self.terminal.regrab()?;
                        self.terminal.0.clear()?;
                    }
                    self.app.state = AppState::None;
                }
                AppState::Pending(does) => {
                    self.terminal.release()?;
                    self.app.state = AppState::None;
                    self.app = does(self.app);
                    self.terminal.regrab()?;
                    self.terminal.0.clear()?;
                }
                AppState::Sourcing(_) => unreachable!(),
            }

            self.terminal.0.draw(|f| self.app.draw(f))?;
            event = cvar.wait(event).unwrap();
        }

        Ok(())
    } // fn run
} // impl AllTheStuff
