mod app;
mod commands;
mod node;
mod tree;
mod view;

use crate::{app::App, tree::Tree, view::View};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use serde_json;
use std::{env::current_dir, error::Error, io};
use tui::{
    backend::{Backend, CrosstermBackend},
    layout::Direction,
    terminal::Terminal,
};

struct TerminalWrap<B: Backend + io::Write>(Terminal<B>);

impl<W: io::Write> TerminalWrap<CrosstermBackend<W>> {
    fn new(mut bla: W) -> Result<TerminalWrap<CrosstermBackend<W>>, Box<dyn Error>> {
        enable_raw_mode()?;
        execute!(bla, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(bla);
        let terminal = Terminal::new(backend)?;
        Ok(TerminalWrap(terminal))
    }
}

impl<B: Backend + io::Write> Drop for TerminalWrap<B> {
    fn drop(&mut self) {
        disable_raw_mode().unwrap();
        execute!(
            self.0.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )
        .unwrap();
        self.0.show_cursor().unwrap();
    }
}

fn main() -> Result<(), Box<dyn Error>> {
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

fn do_event(
    //app_state: &mut AppState
    tree: &mut Tree,
    view: &mut View,
    which: &mut u32,
    split: &mut Option<Direction>,
) -> io::Result<bool> {
    match event::read()? {
        Event::Key(key) => match key.code {
            KeyCode::Char('q') => return Ok(true),

            KeyCode::Char('H') => view.fold(),
            KeyCode::Char('L') => match view.unfold(tree) {
                _ => (),
            },

            KeyCode::Char('h') | KeyCode::Left => view.leave(),
            KeyCode::Char('j') | KeyCode::Down => view.next(),
            KeyCode::Char('k') | KeyCode::Up => view.prev(),
            KeyCode::Char('l') | KeyCode::Right => match view.unfold(tree) {
                Ok(()) => view.enter(),
                Err(_) => (),
            },

            KeyCode::Char(' ') => view.toggle_marked(),

            KeyCode::Char('g') => {
                if let Event::Key(key) = event::read()? {
                    match key.code {
                        KeyCode::Char('g') => view.cursor.clear(),
                        _ => (),
                    }
                }
            }

            KeyCode::Char('w') => {
                if let Event::Key(key) = event::read()? {
                    match key.code {
                        KeyCode::Char('s') => *split = Some(Direction::Vertical),
                        KeyCode::Char('v') => *split = Some(Direction::Horizontal),
                        KeyCode::Char('w') => {
                            if split.is_some() {
                                *which = 1 - *which;
                            }
                        }
                        KeyCode::Char('q') => *split = None,
                        KeyCode::Char('t') => match *split {
                            Some(Direction::Horizontal) => *split = Some(Direction::Vertical),
                            Some(Direction::Vertical) => *split = Some(Direction::Horizontal),
                            None => (),
                        },
                        _ => (),
                    }
                }
            }

            KeyCode::Char('y') => view.offset.scroll -= 1,
            KeyCode::Char('e') => view.offset.scroll += 1,
            KeyCode::Char('Y') => view.offset.shift -= 1,
            KeyCode::Char('E') => view.offset.shift += 1,

            _ => (),
        },

        Event::Mouse(mouse) => match mouse.kind {
            event::MouseEventKind::Down(_) => todo!(),
            // event::MouseEventKind::Up(_) => todo!(),
            // event::MouseEventKind::Drag(_) => todo!(),
            // event::MouseEventKind::Moved => todo!(),
            event::MouseEventKind::ScrollDown => view.offset.scroll += 3,
            event::MouseEventKind::ScrollUp => view.offset.scroll -= 3,
            _ => (),
        },

        _ => (),
    }
    Ok(false)
}
