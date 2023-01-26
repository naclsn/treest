use crate::node;
use crate::tree;
use std::io;
use std::path;

// #[derive(serde::Serialize, serde::Deserialize, Debug)]
#[derive(Debug)]
pub struct View<'tree> {
    cursor: &'tree mut node::Node,
    selection: Vec<&'tree node::Node>,
}

impl View<'_> {
    pub fn new<'tree>(root: &'tree mut tree::Tree) -> View<'tree> {
        View {
            cursor: root.at("".into()).unwrap(),
            selection: vec![],
        }
    }

    pub fn down(mut self, file_name: path::PathBuf) -> io::Result<Self> {
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
