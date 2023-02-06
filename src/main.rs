mod app;
mod commands;
mod line;
mod node;
mod tree;
mod view;

use crate::app::App;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode},
};
use serde_json;
use std::{
    env::current_dir,
    error::Error,
    io::{self, Write},
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
        io::stderr().write(&vec![b'\n'; backend.size()?.height.into()])?;
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
    let res = run_app();
    print!("\r\n");

    match res {
        Ok(_ser) => (), //println!("{ser}"),
        Err(err) => println!("{:?}", err),
    }

    Ok(())
}

fn run_app() -> Result<String, Box<dyn Error>> {
    let mut app = App::new(current_dir()?)?;
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

    Ok(serde_json::to_string(&app)?)
}
