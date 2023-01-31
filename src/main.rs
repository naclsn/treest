mod node;
mod tree;
mod view;

use crate::{tree::Tree, view::View};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::{env::current_dir, error::Error, io};
use tui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
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

    if let Err(err) = res {
        println!("{:?}", err)
    }

    Ok(())
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>) -> io::Result<()> {
    let mut tree = Tree::new(current_dir().unwrap())?;

    let mut view_a = View::new(&mut tree.root);
    view_a.root.unfold(&mut tree.root)?;

    let mut view_b = View::new(&mut tree.root);
    view_b.root.unfold(&mut tree.root)?;

    let mut which = 0;

    loop {
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
                .split(f.size());

            f.render_stateful_widget(&mut tree, chunks[0], &mut view_a);
            f.render_stateful_widget(&mut tree, chunks[1], &mut view_b);
        })?;

        let view = match which {
            0 => &mut view_a,
            1 => &mut view_b,
            _ => panic!(),
        };

        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char('q') => return Ok(()),

                KeyCode::Char('H') => view.fold(),
                KeyCode::Char('L') => view.unfold(&mut tree),

                KeyCode::Char('h') | KeyCode::Left => view.leave(),
                KeyCode::Char('j') | KeyCode::Down => view.next(),
                KeyCode::Char('k') | KeyCode::Up => view.prev(),
                KeyCode::Char('l') | KeyCode::Right => {
                    view.unfold(&mut tree);
                    view.enter();
                }

                KeyCode::Char(' ') => view.toggle_marked(),

                KeyCode::Char('w') => which = 1 - which,

                _ => (),
            } // match key.code
        }
    } // loop term.draw
}
