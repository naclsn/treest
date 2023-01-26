use crate::{node::Node, tree::Tree};
use std::{io, path::PathBuf};

// #[derive(serde::Serialize, serde::Deserialize, Debug)]
#[derive(Debug)]
pub struct View<'tree> {
    cursor: &'tree mut Node,
    selection: Vec<&'tree Node>,
}

impl View<'_> {
    pub fn new<'tree>(root: &'tree mut Tree) -> View<'tree> {
        View {
            cursor: root.at("".into()).unwrap(),
            selection: vec![],
        }
    }

    pub fn down(mut self, file_name: PathBuf) -> io::Result<Self> {
        let ostr = Some(file_name.as_os_str());
        self.cursor = self
            .cursor
            .unfold()?
            .iter_mut()
            .find(|ch| ch.as_path().file_name() == ostr)
            .ok_or(io::Error::from(io::ErrorKind::NotFound))?;
        Ok(self)
    }

    pub fn up(mut self) -> io::Result<Self> {
        self.cursor = todo!("self.cursor.parent()");
    }

    pub fn mark(mut self) {
        self.cursor.mark(true);
        self.selection.push(self.cursor);
        // self
    }
}
