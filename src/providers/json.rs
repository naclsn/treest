use std::cmp::Ordering;
use std::fmt::{Display, Formatter, Result as FmtResult};

use crate::fisovec::FilterSorter;
use crate::tree::Provider;

pub struct Json {}

#[derive(PartialEq)]
pub struct JsonNode {}

impl Display for JsonNode {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        todo!()
    }
}

impl Provider for Json {
    type Fragment = JsonNode;

    fn provide_root(&self) -> Self::Fragment {
        todo!()
    }

    fn provide(&mut self, path: Vec<&Self::Fragment>) -> Vec<Self::Fragment> {
        todo!()
    }
}

impl FilterSorter<JsonNode> for Json {
    fn compare(&self, a: &JsonNode, b: &JsonNode) -> Ordering {
        todo!()
    }

    fn keep(&self, a: &JsonNode) -> bool {
        todo!()
    }
}

impl Json {
    pub fn new(name: String) -> Self {
        _ = name;
        Self {}
    }
}
