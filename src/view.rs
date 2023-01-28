use crate::{node::Node, tree::Tree};
use std::{io, path::PathBuf};

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

    pub fn at_cursor<'a>(&'a self, tree: &'a Tree) -> (&'a Node, &'a State) {
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

    // pub fn down(mut self, file_name: PathBuf) -> io::Result<Self> {
    //     let ostr = Some(file_name.as_os_str());
    //     self.cursor = self
    //         .cursor
    //         .unfold()?
    //         .iter_mut()
    //         .find(|ch| ch.as_path().file_name() == ostr)
    //         .ok_or(io::Error::from(io::ErrorKind::NotFound))?;
    //     Ok(self)
    // }

    // pub fn up(mut self) -> io::Result<Self> {
    //     self.cursor = todo!("self.cursor.parent()");
    // }

    //     pub fn mark(mut self) {
    //         self.cursor.mark(true);
    //         self.selection.push(self.cursor);
    //         // self
    //     }
}
