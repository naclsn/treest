use crate::{node::Node, tree::Tree};
use std::io;

#[derive(Debug)]
pub struct State {
    pub unfolded: bool,
    pub marked: bool,
    pub children: Vec<(usize, State)>,
}

impl State {
    fn new(node: &Node) -> State {
        State {
            unfolded: false,
            marked: false,
            children: node.loaded_children().map_or(vec![], |chs| {
                chs.iter()
                    .enumerate()
                    .map(|(idx, ch)| (idx, State::new(ch)))
                    .collect()
            }),
        }
    }

    pub fn unfold(&mut self, node: &mut Node) -> io::Result<()> {
        if self.children.is_empty() {
            self.children = node.load_children().map(|chs| {
                chs.iter()
                    .enumerate()
                    .map(|(idx, ch)| (idx, State::new(ch)))
                    .collect()
            })?;
        }
        self.unfolded = true;
        Ok(())
    }

    pub fn fold(&mut self) {
        self.unfolded = false;
    }
}

// #[derive(serde::Serialize, serde::Deserialize, Debug)]
#[derive(Debug)]
pub struct View {
    pub root: State,
    pub cursor: Vec<usize>,
    // cursor: &'tree Node,
    // selection: Vec<State>,
}

impl View {
    pub fn new(root: &mut Node) -> View {
        View {
            root: State::new(root),
            cursor: vec![],
            // selection: vec![],
        }
    }

    pub fn at_cursor(&self) -> &State {
        self.cursor
            .iter()
            .fold(&self.root, |acc_state, in_state_idx| {
                &acc_state.children[*in_state_idx].1
            })
    }

    pub fn at_cursor_mut(&mut self) -> &mut State {
        self.cursor
            .iter()
            .fold(&mut self.root, |acc_state, in_state_idx| {
                &mut acc_state.children[*in_state_idx].1
            })
    }

    pub fn at_cursor_pair<'a>(&'a self, tree: &'a Tree) -> (&'a Node, &'a State) {
        self.cursor.iter().fold(
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
        self.cursor.iter().fold(
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
        if self.cursor.is_empty() {
            None
        } else {
            Some(
                self.cursor
                    .iter()
                    .take(self.cursor.len() - 1)
                    .fold(&self.root, |acc_state, in_state_idx| {
                        &acc_state.children[*in_state_idx].1
                    }),
            )
        }
    }

    pub fn enter(&mut self) {
        self.cursor.push(0);
    }

    pub fn leave(&mut self) {
        self.cursor.pop();
    }

    pub fn next(&mut self) {
        if let Some(par) = self.at_parent() {
            let len = par.children.len();
            self.cursor.last_mut().map(|idx| {
                if *idx + 1 < len {
                    *idx += 1
                }
            });
        }
    }

    pub fn prev(&mut self) {
        self.cursor.last_mut().map(|idx| {
            if 0 < *idx {
                *idx -= 1
            }
        });
    }

    pub fn fold(&mut self) {
        self.at_cursor_mut().fold();
    }

    pub fn unfold(&mut self, tree: &mut Tree) {
        let (node, state) = self.at_cursor_pair_mut(tree);
        state.unfold(node).unwrap();
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
}
