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
    style::{Color, Style},
    terminal::Frame,
    widgets::{Block, Borders},
};

#[derive(Serialize, Deserialize, Debug, Clone)]
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
    let (is_focus, next_focus) = if let Some(v) = focus_path {
        (v.is_empty(), 1 == v.len())
    } else {
        (false, false)
    };

    match view_node {
        ViewTree::Leaf(view) => {
            let surround = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Gray));
            if is_focus {
                f.render_widget(surround.clone(), area);
            }
            f.render_stateful_widget(tree, surround.inner(area), view);
        }

        ViewTree::Split(children, dir) => {
            if next_focus {
                let surround = Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray));
                f.render_widget(surround.clone(), area);
            }
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
                let crap = self.bindings.clone(); // XXX: this should not be needed
                let (may, continues) = crap.try_get_action(&self.status.get_pending());

                if let Some(action) = may.clone() {
                    self.status.clear_pending();
                    return action.apply(self, &[]);
                }
                if !continues {
                    self.status.clear_pending();
                }
            } else if let KeyCode::Esc = key.code {
                self.status.clear_pending();
            }
        } else if let Event::Mouse(mouse) = event {
            match mouse.kind {
                _ => (),
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

    fn focused_group(&self) -> Option<&ViewTree> {
        if self.focus.is_empty() {
            return None;
        }
        let len = self.focus.len();
        Some(
            self.focus
                .iter()
                .take(len - 1)
                .fold(&self.views, |acc, idx| {
                    let ViewTree::Split(chs, _) = acc else { unreachable!() };
                    chs.get(*idx).unwrap()
                }),
        )
    }

    fn focused_group_mut(&mut self) -> Option<&mut ViewTree> {
        if self.focus.is_empty() {
            return None;
        }
        let len = self.focus.len();
        Some(
            self.focus
                .iter()
                .take(len - 1)
                .fold(&mut self.views, |acc, idx| {
                    let ViewTree::Split(chs, _) = acc else { unreachable!() };
                    chs.get_mut(*idx).unwrap()
                }),
        )
    }

    pub fn view_split(&mut self, d: Direction) {
        let d = match d {
            Direction::Horizontal => 0,
            Direction::Vertical => 1,
        };

        if self.focus.is_empty() {
            let niw = self.views.clone();
            self.views = ViewTree::Split(vec![self.views.clone(), niw], d);
            self.focus.push(1);
            return;
        }

        let len = self.focus.len();
        let last = self.focus[len - 1];

        let Some(ViewTree::Split(chs, already)) = self.focused_group_mut() else { return; };
        let ViewTree::Leaf(cur) = &chs[last] else { return; };
        let niw = ViewTree::Leaf(cur.clone());

        if d == *already {
            // adding a split within one of same direction:
            // insert it after current and make it current
            chs.insert(last + 1, niw);
            self.focus[len - 1] += 1;
        } else {
            // creating a split of different direction
            // in-place of the current one, that takes
            // the current one as first child
            // (ie. somewhat same as when focus is empty)
            chs[last] = ViewTree::Split(vec![chs[last].clone(), niw], d);
            self.focus.push(1);
        }

        // self
    }

    pub fn view_transpose(&mut self) {
        let gr = self.focused_group_mut();
        match gr {
            Some(ViewTree::Split(_, d)) => *d = 1 - *d,
            _ => (),
        }
    }

    // FIXME: `wvwswhwq`
    pub fn view_close(&mut self) {
        if self.focus.is_empty() {
            return;
        }
        let len = self.focus.len();
        let at = self.focus[len - 1];
        let gr = self.focused_group_mut();
        match gr {
            Some(ViewTree::Split(v, _)) => {
                v.remove(at);
                match v.len() {
                    0 => unreachable!("should not have a split with a single leaf"),
                    1 => {
                        let last = v[0].clone();
                        let was_at = self.focus.pop().unwrap();
                        if let Some(ViewTree::Split(v, _)) = self.focused_group_mut() {
                            v[was_at] = last;
                        } else {
                            self.views = last;
                        }
                    }
                    _ => {
                        if 0 < at {
                            self.focus[len - 1] = at - 1;
                        }
                    }
                }
            }
            _ => (), // YYY: quit on last view close? I prefer no
        }
    }

    // for now movement should only be +1 or -1
    // eg moving 'left' in a d=1(Horizontal) split is +1
    // FIXME: this is still not it: `wswvwswjwj`
    pub fn to_view(&mut self, d: Direction, movement: i8) {
        if self.focus.is_empty() {
            return ();
        }

        let d = match d {
            Direction::Horizontal => 1,
            Direction::Vertical => 0,
        };

        let mut stack = Vec::new();

        // step 1: go down while stacking up, till focus
        // step 2: go up while not good direction or edge
        // step 3: (in while>match>Some..>if) update self.focus

        self.focus.iter().fold(&self.views, |acc, idx| {
            stack.push(acc);
            let ViewTree::Split(chs, _) = acc else { unreachable!() };
            chs.get(*idx).unwrap()
        });

        while {
            match stack.last() {
                Some(ViewTree::Split(v, already)) if d == *already => {
                    let now_focus_len = stack.len();
                    let now_focus_last = self.focus[now_focus_len - 1] as i32 + movement as i32;

                    let fitting = 0 <= now_focus_last && now_focus_last < v.len() as i32;
                    if fitting {
                        let now_focus_last = now_focus_last as usize;
                        self.focus.truncate(now_focus_len);
                        self.focus[now_focus_len - 1] = now_focus_last;

                        // the new focus might not be a leaf yet
                        let mut a = &v[now_focus_last];
                        while let ViewTree::Split(v, dd) = a {
                            // not even sure this is possible
                            a = &v[if d == *dd {
                                let border = if 0 < movement { v.len() - 1 } else { 0 };
                                self.focus.push(border);
                                border
                            } else {
                                self.focus.push(0);
                                0
                            }];
                        }

                        return;
                    }

                    stack.pop();
                    false
                }
                _ => stack.pop().is_some(),
            }
        } {}
    }
}
