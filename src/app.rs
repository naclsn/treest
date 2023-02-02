use crate::{
    commands::CommandMap,
    line::{Line, Status},
    tree::Tree,
    view::View,
};
use crossterm::event::{Event, KeyCode, KeyModifiers};
use serde::{Deserialize, Serialize};
use std::{io, iter, path::PathBuf};
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    terminal::Frame,
    widgets::{Block, Borders},
};

#[derive(Serialize, Deserialize, Debug)]
enum ViewTree {
    Leaf(View),
    Split(Vec<ViewTree>, u8), // XXX: tui::layout::Direction not serializable?
}

#[derive(Serialize, Deserialize)]
pub struct App {
    pub tree: Tree,
    views: ViewTree,
    focus: Vec<usize>,

    #[serde(skip_serializing, skip_deserializing)]
    bindings: CommandMap,
    #[serde(skip_serializing, skip_deserializing)]
    status: Status,
    // pending: Vec<char>,
    #[serde(skip_serializing, skip_deserializing)]
    quit: bool,
}

fn draw_r<B: Backend>(
    view_node: &mut ViewTree,
    tree: &Tree,
    f: &mut Frame<'_, B>,
    area: Rect,
    focus_path: Option<&[usize]>,
) {
    let is_focus = if let Some(v) = focus_path {
        v.is_empty()
    } else {
        false
    };

    match view_node {
        ViewTree::Leaf(view) => {
            let surround = Block::default().borders(Borders::ALL);
            if is_focus {
                f.render_widget(surround.clone(), area);
            }
            f.render_stateful_widget(tree, surround.inner(area), view);
        }

        ViewTree::Split(children, dir) => {
            let chunks = Layout::default()
                .direction(if 0 == *dir {
                    Direction::Vertical
                } else {
                    Direction::Horizontal
                })
                .constraints(
                    children
                        .iter()
                        .map(|_| Constraint::Percentage(100 / children.len() as u16))
                        .collect::<Vec<_>>(),
                )
                .split(area);
            for (idx, (chunk, child)) in iter::zip(chunks, children).enumerate() {
                draw_r(
                    child,
                    tree,
                    f,
                    chunk,
                    focus_path.and_then(|p_slice| {
                        if p_slice.is_empty() {
                            return None;
                        }
                        let (head, tail) = p_slice.split_at(1);
                        if idx == head[0] {
                            return Some(tail);
                        }
                        None
                    }),
                );
            }
        }
    }
}

impl App {
    pub fn new(path: PathBuf) -> io::Result<App> {
        let mut tree = Tree::new(path)?;
        let mut view = View::new(&tree.root);
        view.root.unfold(&mut tree.root)?;
        Ok(App {
            tree,
            views: ViewTree::Leaf(view),
            focus: Vec::new(),
            bindings: CommandMap::default(),
            status: Status::default(),
            // pending: Vec::new(),
            quit: false,
        })
    }

    pub fn set_bindings(&mut self, bindings: CommandMap) {
        self.bindings = bindings;
    }

    pub fn finish(&mut self) {
        self.quit = true;
    }
    pub fn done(&self) -> bool {
        self.quit
    }

    pub fn draw<B: Backend>(&mut self, f: &mut Frame<'_, B>) {
        let main0_line1 = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)].as_ref())
            .split(f.size());
        let (main, line) = (main0_line1[0], main0_line1[1]);

        match &mut self.views {
            ViewTree::Leaf(view) => f.render_stateful_widget(&self.tree, main, view),
            ViewTree::Split(_, _) => {
                draw_r(&mut self.views, &self.tree, f, main, Some(&self.focus))
            }
        }

        let (pair, status) = (
            {
                //self.focused_and_tree() // y this no work! rust?
                let ViewTree::Leaf(r) = self.focus.iter().fold(&self.views, |acc, idx| {
                    let ViewTree::Split(chs, _) = acc else { unreachable!() };
                    chs.get(*idx).unwrap()
                }) else { unreachable!() };
                (r, &self.tree)
            },
            &mut self.status,
        );
        f.render_stateful_widget(Line::new(pair), line, status);
    }

    pub fn do_event(mut self, event: Event) -> App {
        if let Event::Key(key) = event {
            if let KeyCode::Char(c) = key.code {
                // ZZZ: hard-coded for now
                if 'c' == c && key.modifiers.contains(KeyModifiers::CONTROL) {
                    self.finish();
                    return self;
                }

                self.status.push_pending(c);
                let (may, continues) = self.bindings.try_get_action(&self.status.get_pending());

                if let Some(action) = may {
                    self.status.clear_pending();
                    return action(self, &[]);
                }
                if !continues {
                    self.status.clear_pending();
                }
            } else if let KeyCode::Esc = key.code {
                self.status.clear_pending();
            }
        }
        self
    }

    pub fn focused(&self) -> &View {
        let ViewTree::Leaf(r) = self.focus.iter().fold(&self.views, |acc, idx| {
            let ViewTree::Split(chs, _) = acc else { unreachable!() };
            chs.get(*idx).unwrap()
        }) else { unreachable!() };
        r
    }

    pub fn focused_mut(&mut self) -> &mut View {
        let ViewTree::Leaf(r) = self.focus.iter().fold(&mut self.views, |acc, idx| {
            let ViewTree::Split(chs, _) = acc else { unreachable!() };
            chs.get_mut(*idx).unwrap()
        }) else { unreachable!() };
        r
    }

    pub fn focused_and_tree(&self) -> (&View, &Tree) {
        let ViewTree::Leaf(r) = self.focus.iter().fold(&self.views, |acc, idx| {
            let ViewTree::Split(chs, _) = acc else { unreachable!() };
            chs.get(*idx).unwrap()
        }) else { unreachable!() };
        (r, &self.tree)
    }

    pub fn focused_and_tree_mut(&mut self) -> (&mut View, &mut Tree) {
        let ViewTree::Leaf(r) = self.focus.iter().fold(&mut self.views, |acc, idx| {
            let ViewTree::Split(chs, _) = acc else { unreachable!() };
            chs.get_mut(*idx).unwrap()
        }) else { unreachable!() };
        (r, &mut self.tree)
    }

    pub fn split_horizontal(mut self) -> Self {
        let niw = ViewTree::Leaf(self.focused().clone());
        match &mut self.views {
            ViewTree::Split(children, 0) => children.push(niw),
            ViewTree::Leaf(_) | ViewTree::Split(_, _) => {
                self.views = ViewTree::Split(vec![self.views, niw], 0);
                self.focus.push(0);
            }
        }
        self
    }

    pub fn split_vertical(mut self) -> Self {
        let niw = ViewTree::Leaf(self.focused().clone());
        match &mut self.views {
            ViewTree::Split(children, 1) => children.push(niw),
            ViewTree::Leaf(_) | ViewTree::Split(_, _) => {
                self.views = ViewTree::Split(vec![self.views, niw], 1);
                self.focus.push(0);
            }
        }
        self
    }
}
