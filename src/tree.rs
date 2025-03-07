use crate::providers::{Fragment, Provider};

#[derive(Debug, Clone)]
pub struct Node {
    pub fragment: Fragment,
    children: Option<(Vec<Node>, Vec<usize>)>,
    folded: bool,
    marked: bool,
}

#[derive(Debug, Clone)]
pub struct NodePath<'a> {
    pub head: &'a [&'a Node],
    pub tail: &'a Node,
}

impl Node {
    pub fn all_children(&self) -> Option<&[Node]> {
        self.children.as_ref().map(|(nodes, _)| &nodes[..])
    }

    pub fn children(&self) -> Option<Vec<&Node>> {
        self.children
            .as_ref()
            .map(|(nodes, sel)| sel.iter().map(|k| &nodes[*k]).collect())
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
        let nodes: Vec<_> = provider
            .provide(&NodePath {
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
            .collect();

        let mut filtered_sorted: Vec<_> = nodes
            .iter()
            .map(|node| NodePath {
                head: parents,
                tail: node,
            })
            .enumerate()
            .filter(|p| provider.keep(&p.1))
            .collect();
        filtered_sorted.sort_unstable_by(|l, r| provider.order(&l.1, &r.1));
        let sel = filtered_sorted.into_iter().map(|p| p.0).collect();

        self.children = Some((nodes, sel));
        self.folded = false;
    }
}
