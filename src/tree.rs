use std::fmt::{Debug, Display, Formatter, Result as FmtResult};
use std::iter;
use std::ops::Range;

pub trait Provider {
    type Fragment: Debug;

    fn provide_root(&self) -> Self::Fragment;
    fn provide(&self, path: Vec<&Self::Fragment>) -> Vec<Self::Fragment>;
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeRef(usize);

#[derive(Debug)]
pub struct Node<P: Provider> {
    pub fragment: P::Fragment,
    parent: NodeRef,
    children: Option<Range<usize>>,
    folded: bool,
}

#[derive(Debug)]
pub struct Tree<P: Provider> {
    provider: P,
    nodes: Vec<Node<P>>,
}

impl<P: Provider> Node<P> {
    pub fn first_child(&self) -> Option<NodeRef> {
        self.children.as_ref().map(|r| NodeRef(r.start))
    }

    pub fn last_child(&self) -> Option<NodeRef> {
        self.children.as_ref().and_then(|r| {
            if r.is_empty() {
                None
            } else {
                Some(NodeRef(r.end - 1))
            }
        })
    }
}

impl<P: Provider> Tree<P> {
    pub fn new(provider: P) -> Self {
        let fragment = provider.provide_root();
        Self {
            provider,
            nodes: vec![Node {
                fragment,
                parent: NodeRef(0),
                children: None,
                folded: true,
            }],
        }
    }

    pub fn root(&self) -> NodeRef {
        NodeRef(0)
    }

    pub fn at(&self, at: NodeRef) -> &Node<P> {
        &self.nodes[at.0]
    }

    pub fn children_at(&self, at: NodeRef) -> Children<P> {
        Children { tree: self, at }
    }

    pub fn path_at(&self, at: NodeRef) -> Vec<&P::Fragment> {
        let mut cur = at;
        let mut r = Vec::new();

        while NodeRef(0) != cur {
            let node = &self.nodes[cur.0];
            r.push(&node.fragment);
            cur = node.parent;
        }
        r.push(&self.nodes[cur.0].fragment);

        r.reverse();
        r
    }

    pub fn fold_at(&mut self, at: NodeRef) {
        self.nodes[at.0].folded = true;
    }

    pub fn unfold_at(&mut self, at: NodeRef) {
        let children = self
            .provider
            .provide(self.path_at(at))
            .into_iter()
            .map(|fragment| Node {
                fragment,
                parent: at,
                children: None,
                folded: true,
            });

        let start = self.nodes.len();
        self.nodes.extend(children);
        let end = self.nodes.len();

        let node = &mut self.nodes[at.0];
        node.children = Some(start..end);
        node.folded = false;
    }
}

pub struct Children<'a, P: Provider> {
    tree: &'a Tree<P>,
    at: NodeRef,
}

impl<P: Provider> Children<'_, P> {
    pub fn iter(&self) -> impl Iterator<Item = &Node<P>> {
        self.tree
            .at(self.at)
            .children
            .as_ref()
            .unwrap()
            .clone()
            .map(|at| self.tree.at(NodeRef(at)))
    }
}

impl<P: Provider> Display for Node<P>
where
    <P as Provider>::Fragment: Display,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        let depth = f.width().unwrap_or(0);
        let frag = &self.fragment;
        write!(f, "{:1$}{frag}", "", depth * 4)
    }
}

impl<P: Provider> Display for Tree<P>
where
    <P as Provider>::Fragment: Display,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        let mut stack = vec![(0, self.root())];

        while let Some((depth, at)) = stack.pop() {
            let node = self.at(at);
            writeln!(f, "{node:0$}", depth)?;
            if !node.folded {
                // TODO: sort and filter
                let children = node.children.as_ref().unwrap().clone().map(NodeRef).rev();
                stack.extend(iter::repeat(depth + 1).zip(children));
            }

            if 100 < stack.len() {
                break;
            }
        }

        Ok(())
    }
}
