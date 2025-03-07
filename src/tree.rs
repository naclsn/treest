use crate::providers::{Fragment, Provider};

#[derive(Debug, Clone)]
pub struct Node {
    pub fragment: Fragment,
    children: Option<Vec<Node>>,
    folded: bool,
    marked: bool,
}

pub struct NodePath<'a> {
    pub head: &'a [&'a Node],
    pub tail: &'a Node,
}

impl Node {
    pub fn children(&self) -> Option<&Vec<Node>> {
        self.children.as_ref()
    }

    pub fn folded(&self) -> bool {
        self.folded
    }

    pub fn marked(&self) -> bool {
        self.marked
    }
}

impl Node {
    pub fn new() -> Self {
        Self {
            fragment: Fragment(0),
            children: None,
            folded: true,
            marked: false,
        }
    }

    pub fn unfold(&mut self, provider: &mut Box<dyn Provider>, parents: &[&Node]) {
        self.children = Some(
            provider
                .provide(NodePath {
                    head: parents,
                    tail: self,
                })
                .into_iter()
                .map(|fragment| Node {
                    fragment,
                    children: None,
                    folded: true,
                    marked: false,
                })
                .collect(),
        );
        self.folded = false;
    }
}
