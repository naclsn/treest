use std::cmp::Ordering;
use std::borrow::Borrow;
//use std::fmt::{Display, Result as FmtResult, Write};

//use anyhow::Result;
//use thiserror::Error;

//use crate::stabvec::StabVec;
use crate::providers::{Fragment, Provider};

#[derive(Debug, Clone)]
pub struct Node {
    //parent: Option<&Node>,
    pub fragment: Fragment,
    children: Option<Vec<Node>>,
    folded: bool,
    marked: bool,
}

pub struct NodePath<'a> {
    pub head: &'a [&'a Node],
    pub tail: &'a Node,
}

// XXX: make iterators
impl Node {
    //pub fn parent(&self) -> &Node {
    //    self.parent
    //}

    pub fn children(&self) -> Option<&Vec<Node>> {
        self.children.as_ref()
    }

    pub fn folded(&self) -> bool { self.folded }

    pub fn marked(&self) -> bool { self.marked }

    //pub fn iter_parents(&self) -> Vec<&Node> {
    //    let Some(parent) = self.parent else { return Vec::new(); };
    //    let mut r = parent.iter_parents();
    //    r.push(self);
    //    r
    //}
}


//#[allow(dead_code)]
impl Node {
    pub fn new() -> Self {
        Self {
                //parent: None,
                fragment: Fragment(0),
                children: None,
                folded: true,
                marked: false,
        }
    }

    pub fn unfold(&mut self, provider: &mut impl Provider, parents: &[&Node]) {
        self.children = Some(provider.provide(NodePath { head: parents, tail: self }).into_iter().map(|fragment| Node {
            fragment,
            children: None,
            folded: true,
            marked: false,
        }).collect());
        self.folded = false;
    }

    /*
    pub fn root(&self) -> TreePathBuf {
        TreePathBuf {
            head: Vec::new(),
            tail: Fragment(0),
        }
    }
    */

    /*
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
    */
}
