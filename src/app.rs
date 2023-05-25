use crate::{
    commands::{Action, CommandMap, Key},
    line::{Line, Message, Status},
    node::Movement,
    tree::Tree,
    view::View,
};
use crossterm::event::{Event, KeyCode, KeyModifiers};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, io, iter, path::PathBuf};
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    terminal::Frame,
    widgets::{Block, Borders, Clear},
};

#[derive(Serialize, Deserialize, Debug, Clone)]
enum ViewTree {
    Leaf(View),
    Split(Vec<ViewTree>, u8), // XXX: tui::layout::Direction not serializable?
}

#[derive(Serialize, Deserialize)]
struct Internal {
    tree: Tree,
    views: ViewTree,
    focus: Vec<usize>,

    variables: HashMap<String, Vec<String>>,
}

impl Internal {
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
        if let Some(ViewTree::Split(_, d)) = gr {
            *d = 1 - *d;
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
        if let Some(ViewTree::Split(v, _)) = gr {
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
        } //else // YYY: quit on last view close? I prefer no
    }

    pub fn view_close_other(&mut self) {
        self.views = ViewTree::Leaf(self.focused().clone());
        self.focus.clear();
    }

    pub fn focus_to_view_adjacent(&mut self, movement: Movement) {
        let Some(ViewTree::Split(v, _)) = self.focused_group() else { return; };
        let max = v.len();
        if !self.focus.is_empty() {
            if let Some(it) = self.focus.last_mut() {
                match movement {
                    Movement::Forward => {
                        *it += 1;
                        if max <= *it {
                            *it = 0;
                        }
                    }
                    Movement::Backward => {
                        if 0 == *it {
                            *it = max;
                        }
                        *it -= 1
                    }
                }
            }
        }
    }

    // FIXME: this is still not it: `wswvwswjwj`
    pub fn focus_to_view(&mut self, d: Direction, movement: Movement) {
        if self.focus.is_empty() {
            return;
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
                                let border = match movement {
                                    Movement::Forward => v.len() - 1,
                                    Movement::Backward => 0,
                                };
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

    pub fn declare(&mut self, name: &str, value: &[&str]) {
        self.variables.insert(
            name.to_string(),
            value.iter().map(|it| it.to_string()).collect(),
        );
    }

    pub fn lookup(&self, name: &str) -> Vec<String> {
        if name.is_empty() {
            // same as "{.}", ie at cursor full path
            let (view, tree) = self.focused_and_tree();
            let (_, node) = view.at_cursor_pair(tree);
            return vec![node.as_path().to_string_lossy().to_string()];
        }

        match name.split_once('.').unwrap_or((name, "")) {
            (obj @ ("" | "root" | "selection"), ppt) => {
                let (view, tree) = self.focused_and_tree();

                let nodes = match obj {
                    "" => vec![view.at_cursor_pair(tree).1],
                    "root" => vec![&tree.root],
                    "selection" => view
                        .collect_marked_pair(tree)
                        .into_iter()
                        .map(|(_, node)| node)
                        .collect(),
                    //"pwd" => current_dir().unwrap(), // TODO
                    _ => unreachable!(),
                };

                match ppt {
                    "" => nodes
                        .iter()
                        .map(|it| it.as_path().to_string_lossy().to_string())
                        .collect(),
                    "relative" => nodes
                        .iter()
                        .map(|it| {
                            it.as_path()
                                .strip_prefix(tree.root.as_path())
                                .unwrap()
                                .to_string_lossy()
                                .to_string()
                        })
                        .collect(),
                    "file-name" => nodes.iter().map(|it| it.file_name().to_string()).collect(),
                    "extension" => nodes
                        .iter()
                        .map(|it| it.extension().unwrap_or("").to_string())
                        .collect(),
                    _ => panic!("unknown property on object: '{obj}.{ppt}'"),
                }
            }

            (obj, "") => {
                if let Some(it) = self.variables.get(obj) {
                    it.clone()
                } else {
                    vec![]
                }
            }

            _ => todo!("taking propery on arbitrary variables"),
        }
    }

    pub fn var_list(&self) -> Vec<&String> {
        self.variables.keys().collect()
    }
}

#[derive(Default)]
pub enum AppState {
    #[default]
    None,
    Quit,
    Pause,
    Pending(Box<dyn FnOnce(App) -> App>),
    Sourcing(String),
}

#[derive(Serialize, Deserialize)]
pub struct App {
    i: Internal,

    #[serde(skip_serializing, skip_deserializing)]
    bindings: CommandMap,
    #[serde(skip_serializing, skip_deserializing)]
    status: Status,

    #[serde(skip_serializing, skip_deserializing)]
    pub state: AppState,
}

// impl WatchEventHandler for App {
//     fn handle_event(&mut self, event: notify::Result<notify::Event>) {
//         if let Ok(event) = event {
//             println!("Event: {:?}", event);
//         }
//     }
// }

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
        let mut view = View::new(&tree.root)?;
        view.unfold_root(&mut tree)?;
        Ok(App {
            i: Internal {
                tree,
                views: ViewTree::Leaf(view),
                focus: Vec::new(),

                variables: HashMap::new(),
            },

            bindings: CommandMap::default(),
            status: Status::default(),
            state: AppState::None,
        })
    }

    fn fixup_r(vt: &mut ViewTree, ptree: &Tree, tree: &Tree) {
        match vt {
            ViewTree::Leaf(view) => view.fixup(ptree, tree),
            ViewTree::Split(list, _) => {
                for it in list {
                    App::fixup_r(it, ptree, tree);
                }
            }
        }
    }
    pub fn fixup(&mut self) {
        let new = self.i.tree.renew().unwrap();
        App::fixup_r(&mut self.i.views, &self.i.tree, &new);
        self.i.tree = new;
    }

    pub fn rebind(&mut self, key_path: &[Key], action: Action) {
        self.bindings.rebind(key_path, action);
    }

    pub fn get_bindings(&self) -> &CommandMap {
        &self.bindings
    }

    pub fn draw<B: Backend>(&mut self, f: &mut Frame<'_, B>) {
        let main0_line1 = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)].as_ref())
            .split(f.size());
        let (main, line) = (main0_line1[0], main0_line1[1]);

        match &mut self.i.views {
            ViewTree::Leaf(view) => f.render_stateful_widget(&self.i.tree, main, view),
            ViewTree::Split(_, _) => draw_r(
                &mut self.i.views,
                &self.i.tree,
                f,
                main,
                Some(&self.i.focus),
            ),
        }

        let (view, tree) = self.i.focused_and_tree();
        f.render_stateful_widget(Line::new(view, tree), line, &mut self.status);

        if let Some(tb) = self.status.long_message() {
            let h = tb.height();
            let r = if line.y < h {
                f.size()
            } else {
                Rect::new(line.x, line.y - h, line.width, h)
            };
            f.render_widget(Clear, r);
            f.render_widget(tb, r);
        }

        if let Some(s) = self.status.cursor_shift() {
            f.set_cursor(line.x + s, line.y);
        }
    }

    pub fn message(&mut self, message: Message) {
        self.status.message(message);
    }

    pub fn prompt(&mut self, prompt: String, action: Action, initial: Option<&str>) {
        self.status.prompt(prompt, action, initial);
    }

    pub fn do_event(mut self, event: &Event) -> App {
        // ZZZ: hard-coded for now
        if let Event::Key(key) = event {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                match key.code {
                    KeyCode::Char('c') => {
                        self.state = AppState::Quit;
                        return self;
                    }
                    KeyCode::Char('z') => {
                        self.state = AppState::Pause;
                        return self;
                    }
                    _ => (),
                }
            }
        }

        let (prompt_action, event_consumed) =
            self.status.do_event(event, &|name| self.i.lookup(name));
        if let Some((action, args)) = prompt_action {
            return action.apply(self, &args.iter().map(|s| s.as_str()).collect::<Vec<_>>());
        }
        if event_consumed {
            return self;
        }

        if let Event::Key(key) = event {
            if let KeyCode::Esc = key.code {
                self.status.clear_pending();
            } else {
                self.status.push_pending(key.into());
                let crap = self.bindings.clone(); // XXX: this should not be needed
                let (may, continues) = crap.try_get_action(self.status.get_pending());

                if let Some(action) = may {
                    self.status.clear_pending();
                    return action.apply(self, &[]);
                }
                if !continues {
                    self.status.clear_pending();
                }
            }
            // } else if let Event::Mouse(mouse) = event {
            //     match mouse.kind {
            //         _ => (),
            //     }
        }
        self
    }

    pub fn focused(&self) -> &View {
        self.i.focused()
    }

    pub fn focused_mut(&mut self) -> &mut View {
        self.i.focused_mut()
    }

    //pub fn focused_and_tree(&self) -> (&View, &Tree) { self.i.focused_and_tree() }

    pub fn focused_and_tree_mut(&mut self) -> (&mut View, &mut Tree) {
        self.i.focused_and_tree_mut()
    }

    pub fn view_split(&mut self, d: Direction) {
        self.i.view_split(d)
    }

    pub fn view_transpose(&mut self) {
        self.i.view_transpose()
    }

    pub fn view_close(&mut self) {
        self.i.view_close()
    }

    pub fn view_close_other(&mut self) {
        self.i.view_close_other()
    }

    pub fn focus_to_view_adjacent(&mut self, movement: Movement) {
        self.i.focus_to_view_adjacent(movement)
    }

    pub fn focus_to_view(&mut self, d: Direction, movement: Movement) {
        self.i.focus_to_view(d, movement)
    }

    pub fn declare(&mut self, name: &str, value: &[&str]) {
        self.i.declare(name, value)
    }

    pub fn lookup(&self, name: &str) -> Vec<String> {
        self.i.lookup(name)
    }

    pub fn var_list(&self) -> Vec<&String> {
        self.i.var_list()
    }
}
