use crate::{
    node::{Filtering, Movement, Node, Sorting, SortingProp},
    tree::Tree,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, io};

#[derive(Serialize, Deserialize, Debug, Clone)]
struct ViewSettings {
    sort: Sorting,
    filters: Vec<Filtering>,
    reverse: bool,
}

impl ViewSettings {
    fn make_node_state_mapping(&self, chs: &[Node]) -> io::Result<Vec<(usize, State)>> {
        let mut r: Vec<_> = chs
            .iter()
            .filter(|ch| !self.filters.iter().any(|f| f.matches(ch)))
            .enumerate()
            .collect();

        r.sort_unstable_by(|(_, l), (_, r)| Node::cmp_by(l, r, self.sort));

        if self.reverse {
            r.into_iter()
                .rev()
                .map(|(idx, ch)| State::new(ch, self).map(|st| (idx, st)))
                .collect()
        } else {
            r.into_iter()
                .map(|(idx, ch)| State::new(ch, self).map(|st| (idx, st)))
                .collect()
        }
    }

    fn correct_node_state_mapping(
        &self,
        chs: &[Node],
        mut r: Vec<(usize, State)>,
    ) -> Vec<(usize, State)> {
        r.sort_unstable_by(|(lk, _), (rk, _)| Node::cmp_by(&chs[*lk], &chs[*rk], self.sort));

        let iter = r.into_iter().filter(|(k, _)| {
            let ch = &chs[*k];
            !self.filters.iter().any(|f| f.matches(ch))
        });

        if self.reverse {
            iter.rev().collect()
        } else {
            iter.collect()
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct State {
    pub unfolded: bool,
    pub marked: bool,
    pub children: Vec<(usize, State)>,
}

impl State {
    fn new(node: &Node, settings: &ViewSettings) -> io::Result<State> {
        Ok(State {
            unfolded: false,
            marked: false,
            children: node
                .loaded_children()
                .map_or(Ok(Vec::new()), |ok| settings.make_node_state_mapping(ok))?,
        })
    }

    fn renew(
        &self,
        pnode: &Node,
        node: &Node,
        settings: &ViewSettings,
        out_path: &mut Vec<usize>,
        _out_path_cur: usize,
    ) -> State {
        // TODO/FIXME: lsdkdajfieuars
        let out_path_cur = 0;
        if !out_path.is_empty() { out_path.clear(); }

        State {
            unfolded: self.unfolded,
            marked: self.marked,
            children: if !self.unfolded {
                Vec::new()
            } else {
                let Some(plo_chs) = pnode.loaded_children() else { unreachable!() };
                let Some(lo_chs) = node.loaded_children() else { unreachable!() };

                let mut done = HashSet::<&str>::new();

                let previous = &self.children;
                let mut children = Vec::with_capacity(previous.len());
                for (pk, st) in previous {
                    // get the name from the previous tree
                    let pch = &plo_chs[*pk];
                    let name = pch.file_name();
                    done.insert(name);

                    // because the tree might have been renewed,
                    // we try to find nodes by name rather than
                    // relying on the previous mapping
                    let may_matching = lo_chs.iter().position(|ch| ch.file_name() == name);
                    if let Some(k) = may_matching {
                        let ch = &lo_chs[k];
                        let st = st.renew(pch, ch, settings, out_path, out_path_cur + 1);
                        children.push((k, st));
                    }
                } // for in previous

                children.extend(
                    lo_chs
                        .iter()
                        .enumerate()
                        .filter(|(_, ch)| !done.contains(ch.file_name()))
                        .filter_map(|(k, ch)| State::new(ch, settings).map(|st| (k, st)).ok()),
                );

                let children = settings.correct_node_state_mapping(lo_chs, children);

                // if the cursor was on a children of self
                if out_path_cur < out_path.len() {
                    // find where it should land in the new one
                    let (pk, _) = &self.children[out_path[out_path_cur]];
                    let name = plo_chs[*pk].file_name();
                    let may_matching = children
                        .iter()
                        .position(|(k, _)| lo_chs[*k].file_name() == name);
                    if let Some(idx) = may_matching {
                        out_path[out_path_cur] = idx;
                    } else {
                        out_path.truncate(out_path_cur);
                    }
                }

                children
            }, // children: ..
        } // State { .. }
    }

    pub fn visible_height(&self) -> usize {
        if self.unfolded {
            (if 1 == self.children.len() { 0 } else { 1 })
                + self
                    .children
                    .iter()
                    .map(|(_, ch)| ch.visible_height())
                    .sum::<usize>()
        } else {
            1
        }
    }

    fn unfold(&mut self, node: &mut Node, settings: &ViewSettings) -> io::Result<()> {
        if self.children.is_empty() {
            self.children = node
                .load_children()
                .and_then(|v| settings.make_node_state_mapping(v))?;
        }
        self.unfolded = true;
        Ok(())
    }

    pub fn fold(&mut self) {
        self.unfolded = false;
    }

    // special case: movement::backward with skip 0 means starts at -1
    pub fn scan_to(
        &mut self,
        node: &mut Node,
        movement: Movement,
        skip: usize,
        predicate: &mut impl FnMut(&State, &Node) -> ScanToChoice,
        out_path: &mut Vec<usize>,
    ) -> bool {
        if let Some(chs) = node.loaded_children_mut() {
            let base = self.children.iter_mut().enumerate();
            let clo = |arg: (usize, &mut (usize, State))| {
                let (idx, (k, st)) = arg;
                let ch = &mut chs[*k];
                match predicate(st, ch) {
                    ScanToChoice::Break(mark) => {
                        st.marked = mark;
                        out_path.push(idx);
                        Some(true)
                    }
                    ScanToChoice::Continue(mark) => {
                        st.marked = mark;
                        None
                    }
                    ScanToChoice::Recurse => {
                        out_path.push(idx);
                        if st.scan_to(ch, movement, 0, predicate, out_path) {
                            Some(true)
                        } else {
                            out_path.pop();
                            None
                        }
                    }
                    ScanToChoice::Abort(mark) => {
                        st.marked = mark;
                        Some(false)
                    }
                }
            };
            return match movement {
                Movement::Forward => base.skip(skip).filter_map(clo).next(),
                Movement::Backward if 0 == skip => base.take(skip).rev().filter_map(clo).next(),
                Movement::Backward => base.rev().filter_map(clo).next(),
            }
            .unwrap_or(false);
        }
        false
    }

    fn collect_marked<'a>(&'a self, out_res: &mut Vec<&'a State>) {
        if self.marked {
            out_res.push(self);
        }
        for (_, st) in &self.children {
            st.collect_marked(out_res);
        }
    }

    fn collect_marked_pair<'a>(&'a self, node: &'a Node, out_res: &mut Vec<(&'a State, &'a Node)>) {
        if self.marked {
            out_res.push((self, node));
        }
        if let Some(chs) = node.loaded_children() {
            for (k, st) in &self.children {
                st.collect_marked_pair(&chs[*k], out_res);
            }
        }
    }
}

/// used with View::scan_to
pub enum ScanToChoice {
    Break(bool),    // ie. found, stop there (bool = set marked?)
    Continue(bool), // ie. not this one, try next (bool = set marked?)
    Recurse,        // ie. in this node
    Abort(bool),    // ie. give up, stop there (bool = set marked?)
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct Offset {
    pub shift: i32,  // horizontally
    pub scroll: i32, // vertically
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct View {
    pub root: State,
    cursor: Vec<usize>,
    cursor_path_len: usize,
    offset: Offset,
    settings: ViewSettings,
}

impl View {
    pub fn new(root: &Node) -> io::Result<View> {
        let settings = ViewSettings {
            sort: Sorting::new(SortingProp::Name, false),
            filters: Vec::new(),
            reverse: false,
        };
        Ok(View {
            root: State::new(root, &settings)?,
            cursor: Vec::new(),
            cursor_path_len: 0,
            offset: Offset {
                shift: 0,
                scroll: 0,
            },
            // selection: vec![],
            settings,
        })
    }

    pub fn cursor_path(&self) -> &[usize] {
        &self.cursor[..self.cursor_path_len]
    }

    pub fn view_offset(&self) -> Offset {
        self.offset
    }

    pub fn cursor_offset(&self) -> Offset {
        let acc = self
            .cursor_path()
            .iter()
            .fold((&self.root, 1), |(state, acc), idx| {
                let r = 1
                    + acc
                    + state.children[0..*idx]
                        .iter()
                        .map(|(_, ch)| ch.visible_height())
                        .sum::<usize>();
                let s = &state.children[*idx].1;
                (s, r)
            })
            .1;
        let len = self.cursor_path_len as i32;
        Offset {
            shift: len * 4 - self.offset.shift,
            scroll: acc as i32 - self.offset.scroll,
        }
    }

    pub fn cursor_to_root(&mut self) {
        self.cursor_path_len = 0;
    }

    pub fn visible_height(&self) -> usize {
        self.root.visible_height()
    }

    pub fn ensure_cursor_within(&mut self, height: i32, stride: i32) {
        let c_off = self.cursor_offset();
        if c_off.scroll - 1 < stride {
            self.offset.scroll += c_off.scroll - 1 - stride;
        } else if height - stride < c_off.scroll {
            self.offset.scroll += c_off.scroll - 1 - (height - stride - 1);
        }
        self.fit_offset(height);
    }

    pub fn fit_offset(&mut self, _height: i32) {
        if self.offset.scroll < 0 {
            self.offset.scroll = 0;
        } /*else {
              let total = self.visible_height() as i32;
              if height < total && total - height < self.offset.scroll {
                  self.offset.scroll = total - height;
              }
          }*/
    }

    pub fn at_cursor(&self) -> &State {
        self.cursor
            .iter()
            .take(self.cursor_path_len)
            .fold(&self.root, |acc_state, in_state_idx| {
                &acc_state.children[*in_state_idx].1
            })
    }

    pub fn at_cursor_mut(&mut self) -> &mut State {
        self.cursor
            .iter()
            .take(self.cursor_path_len)
            .fold(&mut self.root, |acc_state, in_state_idx| {
                &mut acc_state.children[*in_state_idx].1
            })
    }

    pub fn at_cursor_pair<'a>(&'a self, tree: &'a Tree) -> (&'a State, &'a Node) {
        self.cursor.iter().take(self.cursor_path_len).fold(
            (&self.root, &tree.root),
            |(acc_state, acc_node): (&State, &Node), in_state_idx| {
                let (in_node_idx, next_state) = &acc_state.children[*in_state_idx];
                let next_node = &acc_node
                    .loaded_children()
                    .unwrap()
                    .get(*in_node_idx)
                    .unwrap();
                (next_state, next_node)
            },
        )
    }

    pub fn at_cursor_pair_mut<'a>(
        &'a mut self,
        tree: &'a mut Tree,
    ) -> (&'a mut State, &'a mut Node) {
        self.cursor.iter().take(self.cursor_path_len).fold(
            (&mut self.root, &mut tree.root),
            |(acc_state, acc_node): (&mut State, &mut Node), in_state_idx| {
                let (in_node_idx, next_state) = &mut acc_state.children[*in_state_idx];
                let next_node = acc_node
                    .loaded_children_mut()
                    .unwrap()
                    .get_mut(*in_node_idx)
                    .unwrap();
                (next_state, next_node)
            },
        )
    }

    pub fn at_parent(&self) -> Option<&State> {
        if 0 == self.cursor_path_len {
            None
        } else {
            Some(
                self.cursor
                    .iter()
                    .take(self.cursor_path_len - 1)
                    .fold(&self.root, |acc_state, in_state_idx| {
                        &acc_state.children[*in_state_idx].1
                    }),
            )
        }
    }

    pub fn collect_marked(&self) -> Vec<&State> {
        let mut ret = Vec::new();
        self.root.collect_marked(&mut ret);
        ret
    }

    pub fn collect_marked_pair<'a>(&'a self, tree: &'a Tree) -> Vec<(&'a State, &'a Node)> {
        let mut ret = Vec::new();
        self.root.collect_marked_pair(&tree.root, &mut ret);
        ret
    }

    pub fn enter(&mut self) {
        if !self.at_cursor().children.is_empty() {
            if self.cursor.len() == self.cursor_path_len {
                self.cursor.push(0);
            }
            self.cursor_path_len += 1;
        }
    }

    pub fn leave(&mut self) {
        if 0 < self.cursor_path_len {
            self.cursor_path_len -= 1;
        }
    }

    pub fn next(&mut self) {
        if let Some(par) = self.at_parent() {
            let len = par.children.len();
            self.cursor.truncate(self.cursor_path_len);
            if let Some(idx) = self.cursor.last_mut() {
                if *idx + 1 < len {
                    *idx += 1
                }
            }
        }
    }

    pub fn prev(&mut self) {
        self.cursor.truncate(self.cursor_path_len);
        if let Some(idx) = self.cursor.last_mut() {
            if 0 < *idx {
                *idx -= 1
            }
        }
    }

    // self mutable because moves cursor if finds
    // tree mutable because `ScanToChoice::Recurse` needs to unfold
    pub fn scan_to(
        &mut self,
        tree: &mut Tree,
        movement: Movement,
        predicate: &mut impl FnMut(&State, &Node) -> ScanToChoice,
    ) -> bool {
        let (state, node) = self.at_cursor_pair_mut(tree);
        let mut path = Vec::new();
        if state.scan_to(node, movement, 0, predicate, &mut path) {
            self.cursor.truncate(self.cursor_path_len);
            self.cursor.extend_from_slice(&path);
            self.cursor_path_len = self.cursor.len();
            true
        } else {
            false
        }
    }

    // self mutable because moves cursor if finds
    // tree mutable because `ScanToChoice::Recurse` needs to unfold
    // skips n (at first level only)
    pub fn scan_to_skip(
        &mut self,
        tree: &mut Tree,
        movement: Movement,
        skip: usize,
        predicate: &mut impl FnMut(&State, &Node) -> ScanToChoice,
    ) -> bool {
        let (state, node) = self.at_cursor_pair_mut(tree);
        let mut path = Vec::new();
        if state.scan_to(node, movement, skip, predicate, &mut path) {
            self.cursor.truncate(self.cursor_path_len);
            self.cursor.extend_from_slice(&path);
            self.cursor_path_len = self.cursor.len();
            true
        } else {
            false
        }
    }

    /// re-create the view against the given tree
    /// while trying to keep the same state (eg.
    /// unfolded, selected, cursor...)
    pub fn fixup(&mut self, ptree: &Tree, tree: &Tree) {
        self.root = self
            .root
            .renew(&ptree.root, &tree.root, &self.settings, &mut self.cursor, 0);
        self.cursor_path_len = self.cursor.len();
    }

    pub fn unfold_root(&mut self, tree: &mut Tree) -> io::Result<()> {
        self.root.unfold(&mut tree.root, &self.settings)
    }

    pub fn fold(&mut self) {
        self.at_cursor_mut().fold();
    }

    pub fn unfold(&mut self, tree: &mut Tree) -> io::Result<()> {
        let f_u = self.settings.clone();
        let (state, node) = self.at_cursor_pair_mut(tree);
        state.unfold(node, &f_u)
    }

    pub fn unfolded(&self) -> bool {
        self.at_cursor().unfolded
    }

    pub fn mark(&mut self) {
        self.at_cursor_mut().marked = true;
    }

    pub fn marked(&self) -> bool {
        self.at_cursor().marked
    }

    pub fn toggle_marked(&mut self) {
        let it = self.at_cursor_mut();
        it.marked = !it.marked;
    }

    pub fn set_sorting(&mut self, sort: Sorting, reverse: bool) {
        self.settings.sort = sort;
        self.settings.reverse = reverse;
    }
    pub fn get_sorting(&self) -> (Sorting, bool) {
        (self.settings.sort, self.settings.reverse)
    }
    pub fn add_filtering(&mut self, filter: Filtering) {
        if !self.settings.filters.iter().any(|it| *it == filter) {
            self.settings.filters.push(filter);
        }
    }
    pub fn remove_filtering(&mut self, filter: Filtering) {
        if let Some(found) = self.settings.filters.iter().position(|it| *it == filter) {
            self.settings.filters.remove(found);
        }
    }
    pub fn toggle_filtering(&mut self, filter: Filtering) {
        if let Some(found) = self.settings.filters.iter().position(|it| *it == filter) {
            self.settings.filters.remove(found);
        } else {
            self.settings.filters.push(filter)
        }
    }
    pub fn clear_filtering(&mut self) {
        self.settings.filters.clear();
    }
    pub fn list_filtering(&self) -> &Vec<Filtering> {
        &self.settings.filters
    }
}
