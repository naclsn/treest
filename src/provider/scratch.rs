use std::cmp::Ordering;

use anyhow::Result;

use crate::provider::Provider;
use crate::tree::{Fragment, Node, NodePath};

pub struct Scratch;

impl Provider for Scratch {
    fn provide_root(&self) -> Fragment {
        Box::new("...".to_string())
    }

    fn provide(&mut self, _path: &NodePath) -> Vec<Fragment> {
        Vec::new()
    }

    fn order(&self, left: &NodePath, right: &NodePath) -> Ordering {
        let left: &String = left.tail.fragment();
        let right: &String = right.tail.fragment();
        Ord::cmp(&left, &right)
    }

    fn keep(&self, _path: &NodePath) -> bool {
        true
    }

    fn display(&self, path: &NodePath) -> String {
        path.tail.fragment::<String>().clone()
    }

    fn components(&self, path: &NodePath) -> Vec<String> {
        path.head
            .iter()
            .chain(std::iter::once(&path.tail))
            .map(|n| n.fragment::<String>().clone())
            .collect()
    }

    fn join(&self, components: &[String]) -> String {
        components.join(" ")
    }

    fn split<'a>(&self, path: &'a NodePath<'a>, text: String) -> Result<(Vec<&'a Node>, Fragment)> {
        Ok((path.iter_all().collect(), Box::new(text)))
    }
}

impl Scratch {
    pub fn new(_arg: &str) -> Result<Self> {
        Ok(Self)
    }

    pub fn guess(_arg: &str) -> bool {
        false
    }
}
