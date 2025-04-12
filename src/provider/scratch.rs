use std::cmp::Ordering;

use anyhow::Result;

use crate::provider::{Event, EventPoller, Provider};
use crate::tree::{Fragment, NodePath};

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

    fn compare(&self, in_tree: &Fragment, in_event: &Fragment) -> bool {
        todo!()
    }

    fn request_mk(&mut self, path: &NodePath, text: String) -> Result<Option<Event>> {
        todo!()
    }

    /*
    fn request_cp(&mut self, path: &NodePath, text: String) -> Result<Option<Event>> {
        todo!()
    }

    fn request_rm(&mut self, path: &NodePath) -> Result<Option<Event>> {
        todo!()
    }

    fn request_mv(&mut self, path: &NodePath, text: String) -> Result<Option<Event>> {
        todo!()
    }

    fn request_ch(&mut self, path: &NodePath, text: String) -> Result<Option<Event>> {
        todo!()
    }

    fn request_vi(&mut self, path: &NodePath) -> Result<Vec<String>> {
        todo!()
    }

    fn request_ex(
        &mut self,
        path: &NodePath,
        text: String,
    ) -> Result<(Vec<String>, Option<Event>)> {
        todo!()
    }
    */
}

impl Scratch {
    pub fn new(arg: &str) -> Result<Self> {
        Ok(Self)
    }

    pub fn guess(_arg: &str) -> bool {
        false
    }
}
