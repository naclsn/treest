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
        self.children = node.load_children().map(|chs| {
            chs.iter()
                .enumerate()
                .map(|(idx, ch)| (idx, State::new(ch)))
                .collect()
        })?;
        self.unfolded = true;
        Ok(())
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
                let next_node = &acc_node.loaded_children().unwrap()[*in_node_idx];
                (next_node, next_state)
            },
        )
    }

    pub fn enter(&mut self) {
        self.cursor.push(0);
    }

    pub fn leave(&mut self) {
        self.cursor.pop();
    }

    pub fn next(&mut self) {
        self.cursor.last_mut().map(|idx| *idx += 1);
    }

    pub fn prev(&mut self) {
        self.cursor.last_mut().map(|idx| *idx -= 1);
    }

    pub fn mark(&mut self) {
        self.at_cursor_mut().marked = true;
    }
}
