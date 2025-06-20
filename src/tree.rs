use std::any::Any;
use std::cmp::Ordering;
use std::fmt::{Debug, Formatter, Result as FmtResult};

use crate::provider::Provider;

pub trait FragmentTrait: Any + Send {
    fn as_any(&self) -> &dyn Any;
}
impl<T: Any + Send> FragmentTrait for T {
    fn as_any(&self) -> &dyn Any {
        self
    }
}
pub type Fragment = Box<dyn FragmentTrait>;

impl Debug for Fragment {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{:p}", *self)
    }
}

#[derive(Debug)]
pub struct Node {
    fragment: Fragment,
    children: Option<(Vec<Node>, Vec<usize>)>,
    folded: bool,
    marked: bool,
}

/// basically a slice of &Node with a non-empty guarantee
#[derive(Debug, Clone)]
pub struct NodePath<'a> {
    pub head: &'a [&'a Node],
    pub tail: &'a Node,
}

impl<'a, 'b: 'a> From<&'b [&'a Node]> for NodePath<'a> {
    fn from(value: &'b [&'a Node]) -> Self {
        let k = value.len() - 1;
        Self {
            head: &value[..k],
            tail: value[k],
        }
    }
}

impl<'a, 'b: 'a> From<&'b Vec<&'a Node>> for NodePath<'a> {
    fn from(value: &'b Vec<&'a Node>) -> Self {
        let k = value.len() - 1;
        Self {
            head: &value[..k],
            tail: value[k],
        }
    }
}

impl NodePath<'_> {
    pub fn iter_all(&self) -> impl Iterator<Item = &Node> {
        self.head.iter().copied().chain(std::iter::once(self.tail))
    }
}

impl Node {
    pub fn new(fragment: Fragment) -> Self {
        Self {
            fragment,
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

    pub fn children(&self) -> Option<impl Iterator<Item = &Node>> {
        self.children
            .as_ref()
            .map(|(nodes, sel)| sel.iter().map(|k| &nodes[*k]))
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

    //pub fn iter_recursive(&self) -> Option<impl Iterator<Item = &Node>> {
    //    self.children()
    //        .map(|chs| chs.filter_map(Node::iter_recursive).flatten())
    //}

    /// The returned list will be 1 longer than `path` (think poles and power lines).
    pub fn resolve(&self, index_path: &[usize]) -> Vec<&Node> {
        index_path.iter().fold(vec![self], |mut acc, cur| {
            acc.push(acc.last().unwrap().child(*cur).unwrap());
            acc
        })
    }

    /// The returned list will be 1 shorter than `node_path` (think poles and power lines).
    pub fn unresolve(&self, node_path: &NodePath) -> Vec<usize> {
        let mut it = node_path.iter_all();
        let me = it.next().unwrap();
        assert!(std::ptr::eq(me, self));
        it.fold((Vec::new(), me), |(mut acc, node), cur| {
            acc.push(
                node.children()
                    .unwrap()
                    .position(|child| std::ptr::eq(child, cur))
                    .unwrap(),
            );
            (acc, cur)
        })
        .0
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

    /// Add child node to the target at path.
    pub fn add_child(&mut self, provider: &mut dyn Provider, path: &[usize], child: Node) {
        // XXX: yeah, ik, ill assume i know what im doing; couldnt find a way to express this op
        let parents = self.resolve(path);
        let target = *parents.last().unwrap() as *const _ as *mut Node;

        unsafe {
            let (nodes, sel) = (*target).children.get_or_insert_default();

            let child_path = NodePath {
                head: &parents,
                tail: &child,
            };
            if provider.keep(&child_path) {
                // find the index in `sel` that we need to insert before
                match sel.iter().position(|k| {
                    provider.order(
                        &child_path,
                        &NodePath {
                            head: &parents,
                            tail: &nodes[*k],
                        },
                    ) == Ordering::Less
                }) {
                    Some(pos) => sel.insert(pos, nodes.len()),
                    None => sel.push(nodes.len()), // otherwise it'll be last
                }
            }

            nodes.push(child);
        }
    }

    /// Remove child node from the target at path.
    pub fn remove_child(&mut self, path: &[usize], child: usize) {
        let target = self.resolve_node_mut(path);
        let (nodes, sel) = target.children.as_mut().unwrap();
        let pk = sel.remove(child);
        nodes.swap_remove(pk);
        // adjust indices: with swap_remove, the last child changes index
        // if it was kept (Provider::keep) then find and update it
        if let Some(k) = sel.iter_mut().find(|k| nodes.len() == **k) {
            *k = pk;
        }
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
        provider: &mut dyn Provider,
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
