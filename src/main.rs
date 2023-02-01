mod node;
mod tree;
mod view;

use crate::{tree::Tree, view::View};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use serde_json;
use std::{env::current_dir, error::Error, io};
use tui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    widgets::{Block, Borders},
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
    let mut tree = Tree::new(current_dir().unwrap())?;

    let mut view_a = View::new(&mut tree.root);
    view_a.root.unfold(&mut tree.root)?;

    let mut view_b = View::new(&mut tree.root);
    view_b.root.unfold(&mut tree.root)?;

    let mut which = 0;
    let mut split: Option<Direction> = None;

    loop {
        terminal.draw(|f| match split.clone() {
            Some(direction) => {
                let chunks = Layout::default()
                    .direction(direction)
                    .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
                    .split(f.size());

                let surr_a = Block::default().borders(Borders::ALL);
                if 0 == which {
                    f.render_widget(surr_a.clone(), chunks[0]);
                }
                f.render_stateful_widget(&mut tree, surr_a.inner(chunks[0]), &mut view_a);

                let surr_b = Block::default().borders(Borders::ALL);
                if 1 == which {
                    f.render_widget(surr_b.clone(), chunks[1]);
                }
                f.render_stateful_widget(&mut tree, surr_b.inner(chunks[1]), &mut view_b);
            }
            None => {
                f.render_stateful_widget(&mut tree, f.size(), &mut view_a);
            }
        })?;

        let view = match which {
            0 => &mut view_a,
            1 => &mut view_b,
            _ => unreachable!(),
        };

        if do_event(&mut tree, view, &mut which, &mut split)? {
            break;
        }
    } // loop

    return Ok(format!("tree: {}\n", serde_json::to_string(&tree)?)
        + &format!("view_a: {}\n", serde_json::to_string(&view_a)?)
        + &format!("view_b: {}\n", serde_json::to_string(&view_b)?));
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
