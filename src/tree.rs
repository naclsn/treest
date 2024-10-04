use std::fmt::{Display, Result as FmtResult, Write};

use anyhow::Result;
use thiserror::Error;

use crate::fisovec::{FilterSorter, FisoVec};
use crate::stabvec::StabVec;

pub trait Fragment: PartialEq {}
impl<T: PartialEq> Fragment for T {}

/// A type that is able to provide a tree structure.
pub trait Provider: FilterSorter<Self::Fragment> {
    /// The fragment type essentially correspond to a path component. A sequence of fragments (eg.
    /// `Vec<&Fragment>`) locates a node in the tree. Fragment must implement `PartialEq`, and
    /// usually also implement `Display` (see also `ProviderExt`).
    type Fragment: Fragment;

    /// Provide the root of the tree. This is usualy an enum unit type.
    /// Note that mutations are disable here.
    fn provide_root(&self) -> Self::Fragment;

    /// Provide/generate the children nodes for the node at the given path.
    /// Note that the path is never empty (at least the root).
    fn provide(&mut self, path: &[&Self::Fragment]) -> Vec<Self::Fragment>;
}

/// Extra things, every functions are defaulted.
pub trait ProviderExt: Provider
where
    Self::Fragment: Display,
{
    /// For the navigation view, this is the path shown at the bottom.
    fn write_nav_path(&self, f: &mut impl Write, path: &[&Self::Fragment]) -> FmtResult {
        path.iter().try_for_each(|it| write!(f, "{it}"))
    }

    /// This is the path as expanded when a '%' is found in command args.
    fn write_arg_path(&self, f: &mut impl Write, path: &[&Self::Fragment]) -> FmtResult {
        path.iter().try_for_each(|it| write!(f, "{it}"))
    }

    /// lskdjf
    fn command(&mut self, cmd: &[String]) -> Result<String> {
        Err(CmdError::NotACommand(cmd[0].clone()).into())
    }
}

#[derive(Error, Debug)]
pub enum CmdError {
    #[error("not a command: {0}")]
    NotACommand(String),
    #[error("incorrect number arguments for {name}: {given} given, expected {expected}")]
    WrongArgCount {
        name: String,
        given: usize,
        expected: usize,
    },
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
}

#[allow(dead_code)]
impl<P: Provider> Tree<P> {
    pub fn new(provider: P) -> Self {
        let fragment = provider.provide_root();
        Self {
            provider,
            nodes: FromIterator::from_iter(vec![Node::new(fragment, NodeRef(0))]),
        }
    }

    pub(crate) fn provider(&self) -> &P {
        &self.provider
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
            .provide(&self.path_at(at))
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
            .provide(&self.path_at(at))
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

impl<P: ProviderExt> Tree<P>
where
    <P as Provider>::Fragment: Display,
{
    pub fn provider_command(&mut self, cmd: &[String]) -> Result<String> {
        self.provider.command(cmd)
    }
}
