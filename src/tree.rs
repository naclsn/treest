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
    pub fn new() -> Self {
        Self {
            fragment: Fragment(0),
            children: None,
            folded: true,
            marked: false,
        }
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

    /// res will be 1 shorter than `path` (poles and power lines)
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

    pub fn unfold(&mut self, provider: &mut Box<dyn Provider>, path: &[usize]) -> usize {
        let parents = self.resolve(path);

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
        let unfolded = self.resolve_node_mut(path);
        unfolded.children = Some((nodes, sel));
        unfolded.folded = false;
        r
    }
}
