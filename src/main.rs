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
    panic,
};
use tui::{
    backend::{Backend, CrosstermBackend},
    terminal::Terminal,
};

struct TerminalWrap<B: Backend + io::Write>(Terminal<B>);

impl<W: io::Write> TerminalWrap<CrosstermBackend<W>> {
    fn new(mut bla: W) -> Result<TerminalWrap<CrosstermBackend<W>>, Box<dyn Error>> {
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
}

impl<B: Backend + io::Write> Drop for TerminalWrap<B> {
    fn drop(&mut self) {
        disable_raw_mode().unwrap();
        execute!(
            self.0.backend_mut(),
            //LeaveAlternateScreen,
            DisableMouseCapture
        )
        .unwrap();
        self.0.show_cursor().unwrap();
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    // panic::set_hook(Box::new(|_| disable_raw_mode().unwrap()));

    let mut terminal = TerminalWrap::new(io::stderr())?;
    let res = run_app(&mut terminal.0);
    drop(terminal);

    match res {
        Ok(_ser) => (), //println!("{ser}"),
        Err(err) => println!("{:?}", err),
    }

    Ok(())
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>) -> io::Result<String> {
    let mut app = App::new(current_dir()?)?;

    while !app.done() {
        terminal.draw(|f| app.draw(f))?;
        app = app.do_event(event::read()?);
    }

    Ok(serde_json::to_string(&app)?)
}
