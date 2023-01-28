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
use std::{
    env::{args_os, current_dir},
    error::Error,
    io,
};
use tui::{
    backend::{Backend, CrosstermBackend},
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, BorderType, Borders},
    Frame, Terminal,
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
    let mut tree = Tree::new(current_dir().unwrap()).expect("could not unfold root");
    // let views = vec![View::new()];
    let mut view = View::new(&mut tree.root);

    // NOTE: (temporary obvsly) opens root, first child and it first child too
    // (this works on my setup because at 0 is `.git/` and at 0 of it is `hooks/`)
    view.root.unfold(&mut tree.root)?;
    view.root.children[0]
        .1
        .unfold(tree.root.loaded_children_mut().unwrap().get_mut(0).unwrap())?;
    view.root.children[0].1.children[0].1.unfold(
        tree.root
            .loaded_children_mut()
            .unwrap()
            .get_mut(0)
            .unwrap()
            .loaded_children_mut()
            .unwrap()
            .get_mut(0)
            .unwrap(),
    )?;

    view.enter(); // enter root (cursor is on `.git/`)
    view.enter(); // enter `.git/` (cursor is on `.git/hooks`)
    view.next(); // move to next child of `.git/` (on my setup this was `.git/info/`)

    loop {
        terminal.draw(|f| {
            // ui(f, tree);
            f.render_stateful_widget(&mut tree, f.size(), &mut view);
        })?;

        if let Event::Key(key) = event::read()? {
            if let KeyCode::Char('q') = key.code {
                return Ok(());
            }
        }
    }
}

fn ui<B: Backend>(f: &mut Frame<B>, t: Tree) -> Tree {
    let size = f.size();

    let block = Block::default()
        .borders(Borders::ALL)
        .title("heyo")
        .title_alignment(Alignment::Center)
        .border_type(BorderType::Rounded);
    f.render_widget(block, size);

    todo!()
}
