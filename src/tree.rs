use std::any::Any;
use std::fmt::{Debug, Formatter, Result as FmtResult};

use crate::providers::Provider;

pub trait FragmentTrait: Any + Send {
    fn as_any(&self) -> &dyn Any;
}
impl<T: Any + Send> FragmentTrait for T {
    fn as_any(&self) -> &dyn Any {
        self
    }
}
pub type Fragment = Box<dyn FragmentTrait>;

//#[derive(Debug)]
pub struct Node {
    fragment: Fragment,
    children: Option<(Vec<Node>, Vec<usize>)>,
    folded: bool,
    marked: bool,
}

impl Debug for Node {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("Node")
            .field("fragment", &format!("{:p}", self.fragment))
            .field("children", &self.children)
            .field("folded", &self.folded)
            .field("marked", &self.marked)
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct NodePath<'a> {
    pub head: &'a [&'a Node],
    pub tail: &'a Node,
}

impl<'a> From<&'a [&'a Node]> for NodePath<'a> {
    fn from(value: &'a [&'a Node]) -> Self {
        let k = value.len() - 1;
        Self {
            head: &value[..k],
            tail: value[k],
        }
    }
}

impl Node {
    pub fn new(root: Fragment) -> Self {
        Self {
            fragment: root,
            children: None,
            folded: true,
            marked: false,
        }
    }

    pub fn fragment<T: 'static>(&self) -> &T {
        (*self.fragment).as_any().downcast_ref().unwrap()
    }

    pub fn is_loaded(&self) -> bool {
        self.children.is_some()
    }

    pub fn is_folded(&self) -> bool {
        self.folded
    }

    pub fn is_marked(&self) -> bool {
        self.marked
    }

    pub fn set_folded(&mut self, is: bool) {
        self.folded = is;
    }

    pub fn set_marked(&mut self, is: bool) {
        self.marked = is;
    }

    pub fn children(&self) -> Option<Vec<&Node>> {
        self.children
            .as_ref()
            .map(|(nodes, sel)| sel.iter().map(|k| &nodes[*k]).collect())
    }

    pub fn child_count(&self) -> usize {
        self.children
            .as_ref()
            .map(|(_, sel)| sel.len())
            .unwrap_or(0)
    }

    pub fn child(&self, nth: usize) -> Option<&Node> {
        self.children.as_ref().map(|(nodes, sel)| &nodes[sel[nth]])
    }

    pub fn child_mut(&mut self, nth: usize) -> Option<&mut Node> {
        self.children
            .as_mut()
            .map(|(nodes, sel)| &mut nodes[sel[nth]])
    }

    /// The returned list will be 1 longer than `path` (think poles and power lines).
    pub fn resolve(&self, path: &[usize]) -> Vec<&Node> {
        path.iter().fold(vec![self], |mut acc, cur| {
            acc.push(acc.last().unwrap().child(*cur).unwrap());
            acc
        })
    }

    pub fn resolve_node(&self, path: &[usize]) -> &Node {
        path.iter().fold(self, |acc, cur| acc.child(*cur).unwrap())
    }

    pub fn resolve_node_mut(&mut self, path: &[usize]) -> &mut Node {
        path.iter()
            .fold(self, |acc, cur| acc.child_mut(*cur).unwrap())
    }

    /// To use over `resolve_node` when the path cannot be trusted.
    pub fn try_resolve_node(&self, path: &[usize]) -> Option<&Node> {
        path.iter().try_fold(self, |acc, cur| acc.child(*cur))
    }

    /// To use over `resolve_node_mut` when the path cannot be trusted.
    pub fn try_resolve_node_mut(&mut self, path: &[usize]) -> Option<&mut Node> {
        path.iter().try_fold(self, |acc, cur| acc.child_mut(*cur))
    }

    /// Load the child nodes for the target at path.
    ///
    /// `folded` indicates whether to actually unfold the node.
    ///
    /// If these where already loaded and `reload` is not true,
    /// this is equivalent to `target.set_folded(folded)`.
    ///
    /// The number of (visible) children is always returned.
    pub fn load(
        &mut self,
        provider: &mut Box<dyn Provider>,
        path: &[usize],
        reload: bool,
        folded: bool,
    ) -> usize {
        let parents = self.resolve(path);

        if !reload {
            let target = parents.last().unwrap();
            if target.is_loaded() {
                if target.is_folded() != folded {
                    let target = self.resolve_node_mut(path);
                    target.set_folded(folded);
                    return target.child_count();
                } else {
                    return target.child_count();
                }
            }
        }

        let nodes: Vec<_> = provider
            .provide(&parents[..].into())
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
                head: &parents,
                tail: node,
            })
            .enumerate()
            .filter(|p| provider.keep(&p.1))
            .collect();
        filtered_sorted.sort_unstable_by(|l, r| provider.order(&l.1, &r.1));
        let sel: Vec<_> = filtered_sorted.into_iter().map(|p| p.0).collect();

        let r = sel.len();
        let target = self.resolve_node_mut(path);
        target.children = Some((nodes, sel));
        target.set_folded(folded);
        r
    }
}

#[cfg(test)]
#[macro_export]
macro_rules! make_test_tree {
    ($frag:literal $folded:literal $marked:literal [$(
        $cfrag:literal $cfolded:literal $cmarked:literal $cchildren:tt
    )*]) => {
        crate::tree::Node::test_new(
            crate::providers::Fragment($frag),
            Some(vec![$(
                crate::make_test_tree!($cfrag $cfolded $cmarked $cchildren),
            )*]),
            $folded,
            $marked,
        )
    };

    ($frag:literal $folded:literal $marked:literal -) => {
        crate::tree::Node::test_new(
            crate::providers::Fragment($frag),
            None,
            $folded,
            $marked,
        )
    };
}

#[cfg(test)]
impl Node {
    pub fn test_new(
        fragment: Fragment,
        children: Option<Vec<Node>>,
        folded: bool,
        marked: bool,
    ) -> Self {
        Self {
            fragment,
            children: children.map(|v| {
                let len = v.len();
                (v, (0..len).collect())
            }),
            folded,
            marked,
        }
    }
}
