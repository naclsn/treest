use crate::fisovec::{FilterSorter, FisoVec};
use crate::stabvec::StabVec;

pub trait Fragment: PartialEq {}
impl<T: PartialEq> Fragment for T {}

pub trait Provider: FilterSorter<Self::Fragment> {
    type Fragment: Fragment;

    fn provide_root(&self) -> Self::Fragment;
    fn provide(&mut self, path: Vec<&Self::Fragment>) -> Vec<Self::Fragment>;
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeRef(usize);

#[derive(Debug, Clone)]
pub struct Node<F: Fragment> {
    pub fragment: F,
    parent: NodeRef,
    children: Option<FisoVec<NodeRef>>,
    folded: bool,
    marked: bool,
}

#[derive(Debug)]
pub struct Tree<P: Provider> {
    provider: P,
    nodes: StabVec<Node<P::Fragment>>,
}

impl<F: Fragment> Node<F> {
    fn new(fragment: F, parent: NodeRef) -> Self {
        Self {
            fragment,
            parent,
            children: None,
            folded: true,
            marked: false,
        }
    }

    pub fn parent(&self) -> NodeRef {
        self.parent
    }

    pub fn children(&self) -> Option<&FisoVec<NodeRef>> {
        self.children.as_ref()
    }

    pub fn folded(&self) -> bool {
        self.folded
    }

    pub fn marked(&self) -> bool {
        self.marked
    }

    /*
    pub fn first_child(&self) -> Option<NodeRef> {
        self.children.as_ref().and_then(|v| v.first()).copied()
    }

    pub fn last_child(&self) -> Option<NodeRef> {
        self.children.as_ref().and_then(|v| v.last()).copied()
    }
    */
}

impl<P: Provider> Tree<P> {
    pub fn new(provider: P) -> Self {
        let fragment = provider.provide_root();
        Self {
            provider,
            nodes: FromIterator::from_iter(vec![Node::new(fragment, NodeRef(0))]),
        }
    }

    pub fn root(&self) -> NodeRef {
        NodeRef(0)
    }

    pub fn marked(&self) -> impl Iterator<Item = NodeRef> + '_ {
        self.nodes
            .iter_ref()
            .filter_map(|(k, n)| if n.marked { Some(NodeRef(k)) } else { None })
    }

    pub fn at(&self, at: NodeRef) -> &Node<P::Fragment> {
        &self.nodes[at.0]
    }

    fn at_mut(&mut self, at: NodeRef) -> &mut Node<P::Fragment> {
        &mut self.nodes[at.0]
    }

    pub fn path_at(&self, at: NodeRef) -> Vec<&P::Fragment> {
        let mut cur = at;
        let mut r = Vec::new();

        while NodeRef(0) != cur {
            let node = self.at(cur);
            r.push(&node.fragment);
            cur = node.parent;
        }
        r.push(&self.at(cur).fragment);

        r.reverse();
        r
    }

    pub fn filter_sort_at(&mut self, at: NodeRef) {
        let meme = unsafe { &mut *(self as *mut Self) };
        if let Some(ch) = &mut self.at_mut(at).children {
            ch.map_filter_sort(meme, |me, r| &me.at(*r).fragment, &meme.provider);
        }
    }

    pub fn fold_at(&mut self, at: NodeRef) {
        self.at_mut(at).folded = true;
    }

    pub fn unfold_at(&mut self, at: NodeRef) {
        let node = self.at_mut(at);
        if node.children.is_some() {
            node.folded = false;
            return;
        }

        let mut children: FisoVec<_> = unsafe { &mut *(&mut self.provider as *mut P) }
            .provide(self.path_at(at))
            .into_iter()
            .map(|fragment| NodeRef(self.nodes.insert(Node::new(fragment, at))))
            .collect();
        children.map_filter_sort(self, |me, r| &me.at(*r).fragment, &self.provider);

        let node = self.at_mut(at);
        node.children = Some(children);
        node.folded = false;
    }

    pub fn remove_at(&mut self, at: NodeRef) {
        if NodeRef(0) == at {
            return;
        }

        if let Some(mut removed) = self.nodes.remove(at.0) {
            if let Some(v) = removed.children.take() {
                for child in v.into_inner() {
                    self.remove_at(child);
                }
            }

            let in_parent = self.at_mut(removed.parent).children.as_mut().unwrap();
            let me = in_parent
                .as_mut()
                .iter_mut()
                .position(|c| at == *c)
                .unwrap();
            in_parent.inner_remove(me);
        }
    }

    pub fn update_at(&mut self, at: NodeRef) {
        let node = self.at_mut(at);
        let Some(mut prev_refs) = node.children.take().map(FisoVec::into_inner) else {
            return;
        };
        if node.folded {
            for child in prev_refs {
                self.remove_at(child);
            }
            return;
        }

        let mut children: FisoVec<_> = unsafe { &mut *(&mut self.provider as *mut P) }
            .provide(self.path_at(at))
            .into_iter()
            .map(|fragment| {
                let searched = prev_refs
                    .iter()
                    .position(|k| self.at(*k).fragment == fragment)
                    .map(|k| prev_refs.swap_remove(k));
                let replace = Node::new(fragment, at);

                if let Some(found) = searched {
                    self.update_at(found);
                    self.nodes.replace(found.0, replace);
                    found
                } else {
                    NodeRef(self.nodes.insert(replace))
                }
            })
            .collect();
        children.map_filter_sort(self, |me, r| &me.at(*r).fragment, &self.provider);

        self.at_mut(at).children = Some(children);
    }

    pub fn toggle_mark_at(&mut self, at: NodeRef) {
        let node = self.at_mut(at);
        node.marked = !node.marked;
    }
}
