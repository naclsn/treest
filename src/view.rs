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
    // cursor: &'tree Node,
    // selection: Vec<State>,
}

impl View {
    pub fn new(root: &mut Node) -> View {
        View {
            root: State::new(root),
            // selection: vec![],
        }
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
