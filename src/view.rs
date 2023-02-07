use crate::{
    node::{Filtering, Node, Sorting, SortingProp},
    tree::Tree,
};
use serde::{Deserialize, Serialize};
use std::io;

#[derive(Serialize, Deserialize, Debug, Clone)]
struct ViewSettings {
    sort: Sorting,
    filter: Filtering,
    reverse: bool,
}

impl ViewSettings {
    fn make_node_state_mapping(&self, chs: &Vec<Node>) -> io::Result<Vec<(usize, State)>> {
        let mut r: Vec<_> = chs
            .iter()
            .filter(|_| true) // TODO
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
        chs: &Vec<Node>,
        mut r: Vec<(usize, State)>,
    ) -> Vec<(usize, State)> {
        r.sort_unstable_by(|(lk, _), (rk, _)| Node::cmp_by(&chs[*lk], &chs[*rk], self.sort));

        let iter = r.into_iter().filter(|_| true); // TODO

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

    fn renew(&self, node: &Node, settings: &ViewSettings) -> io::Result<State> {
        Ok(State {
            unfolded: self.unfolded,
            marked: self.marked,
            children: if !self.unfolded {
                Vec::new()
            } else {
                let Some(ok) = node.loaded_children() else { unreachable!() };
                settings.correct_node_state_mapping(
                    ok,
                    self.children
                        .iter()
                        .map(|(k, st)| st.renew(&ok[*k], settings).map(|st| (*k, st)))
                        .collect::<io::Result<_>>()?,
                )
            },
        })
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
    // selection: Vec<State>,
    settings: ViewSettings,
}

impl View {
    pub fn new(root: &Node) -> io::Result<View> {
        let settings = ViewSettings {
            sort: Sorting::new(SortingProp::Name, false),
            filter: Filtering::None,
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
            .cursor
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

    pub fn at_cursor_pair<'a>(&'a self, tree: &'a Tree) -> (&'a Node, &'a State) {
        self.cursor.iter().take(self.cursor_path_len).fold(
            (&tree.root, &self.root),
            |(acc_node, acc_state): (&Node, &State), in_state_idx| {
                let (in_node_idx, next_state) = &acc_state.children[*in_state_idx];
                let next_node = &acc_node
                    .loaded_children()
                    .unwrap()
                    .get(*in_node_idx)
                    .unwrap();
                (next_node, next_state)
            },
        )
    }

    pub fn at_cursor_pair_mut<'a>(
        &'a mut self,
        tree: &'a mut Tree,
    ) -> (&'a mut Node, &'a mut State) {
        self.cursor.iter().take(self.cursor_path_len).fold(
            (&mut tree.root, &mut self.root),
            |(acc_node, acc_state): (&mut Node, &mut State), in_state_idx| {
                let (in_node_idx, next_state) = &mut acc_state.children[*in_state_idx];
                let next_node = acc_node
                    .loaded_children_mut()
                    .unwrap()
                    .get_mut(*in_node_idx)
                    .unwrap();
                (next_node, next_state)
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
            self.cursor.last_mut().map(|idx| {
                if *idx + 1 < len {
                    *idx += 1
                }
            });
        }
    }

    pub fn prev(&mut self) {
        self.cursor.truncate(self.cursor_path_len);
        self.cursor.last_mut().map(|idx| {
            if 0 < *idx {
                *idx -= 1
            }
        });
    }

    pub fn renew_root(&mut self, tree: &Tree) -> io::Result<()> {
        self.root = self.root.renew(&tree.root, &self.settings)?;
        // TODO: try to update accordingly
        self.cursor.clear();
        self.cursor_path_len = 0;
        Ok(())
    }

    pub fn unfold_root(&mut self, tree: &mut Tree) -> io::Result<()> {
        self.root.unfold(&mut tree.root, &self.settings)
    }

    pub fn fold(&mut self) {
        self.at_cursor_mut().fold();
    }

    pub fn unfold(&mut self, tree: &mut Tree) -> io::Result<()> {
        let f_u = self.settings.clone();
        let (node, state) = self.at_cursor_pair_mut(tree);
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
}
