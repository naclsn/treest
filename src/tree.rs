use std::fmt::Debug;

pub trait Provider {
    type Data: Debug;
    type Fragment: Debug;

    fn provide(path: Vec<&Self::Fragment>) -> Vec<(Self::Data, Self::Fragment)>;
}

#[derive(Debug, Clone, Copy)]
pub struct NodeRef(usize);

#[derive(Debug)]
pub struct Node<P: Provider> {
    pub data: P::Data,
    pub fragment: P::Fragment,
    parent: Option<NodeRef>,
    children: Vec<NodeRef>,
    folded: bool,
}

#[derive(Debug)]
pub struct Tree<P: Provider> {
    provider: P,
    nodes: Vec<Node<P>>,
}

impl<P: Provider> Tree<P> {
    pub fn new(provider: P, root: (P::Data, P::Fragment)) -> Self {
        Self {
            provider,
            nodes: vec![Node {
                data: root.0,
                fragment: root.1,
                parent: None,
                children: Vec::new(),
                folded: true,
            }],
        }
    }

    pub fn at(&self, at: NodeRef) -> &Node<P> {
        &self.nodes[at.0]
    }

    pub fn children_at(&self, at: NodeRef) -> Children<P> {
        todo!()
    }

    pub fn path_at(&self, at: NodeRef) -> Vec<&P::Fragment> {
        let mut cur = &self.nodes[at.0];
        let mut r = vec![&cur.fragment];

        while let Some(p) = &cur.parent {
            cur = self.at(*p);
            r.push(&cur.fragment);
        }

        r.reverse();
        r
    }

    pub fn fold_at(&mut self, at: NodeRef) {
        self.nodes[at.0].folded = true;
    }

    pub fn unfold_at(&mut self, at: NodeRef) {
        self.nodes.extend(
            P::provide(self.path_at(at))
                .into_iter()
                .map(|(data, fragment)| Node {
                    data,
                    fragment,
                    parent: Some(at),
                    children: Vec::new(),
                    folded: true,
                }),
        );

        self.nodes[at.0].folded = false;
    }

    //pub fn iter(&self) -> impl Iterator<Item = &Tree<P>> {
    //    self.children.iter().map(|c| app.at(c))
    //}

    //pub fn iter_mut<'a>(&'a self, app: &'a mut App<P>) -> impl Iterator<Item = &mut Tree<P>> {
    //    self.children.iter().map(|c| app.at_mut(c))
    //}
}
