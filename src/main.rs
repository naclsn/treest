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
    Terminal,
};

fn main() -> Result<(), Box<dyn Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = run_app(&mut terminal);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    match res {
        Ok(ser) => println!("{ser}"),
        Err(err) => println!("{:?}", err),
    }

    Ok(())
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>) -> io::Result<String> {
    let mut app = App::new(current_dir().unwrap())?;

    loop {
        terminal.draw(|f| app.draw(f))?;

        // if do_event(&mut tree, view, &mut which, &mut split)? {
        //     break;
        // }

        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char('q') => break,

                KeyCode::Char('w') => {
                    if let Event::Key(key) = event::read()? {
                        match key.code {
                            KeyCode::Char('s') => app = app.split_horizontal(),
                            KeyCode::Char('v') => app = app.split_vertical(),
                            _ => (),
                        }
                    }
                }

                _ => (),
            }
        }
    } // loop

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
                        KeyCode::Char('g') => view.cursor = Vec::new(),
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
