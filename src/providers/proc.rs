use std::cmp::Ordering;
use std::fmt::{Display, Formatter, Result as FmtResult};

use anyhow::Result;
use thiserror::Error;

use crate::fisovec::FilterSorter;
use crate::tree::{Provider, ProviderExt};

pub struct Proc;

#[derive(Error, Debug)]
pub enum ProcProviderError {}

#[derive(PartialEq)]
pub struct ProcNode;

impl Display for ProcNode {
    fn fmt(&self, _f: &mut Formatter<'_>) -> FmtResult {
        todo!()
    }
}

impl Provider for Proc {
    type Fragment = ProcNode;

    fn provide_root(&self) -> Self::Fragment {
        todo!()
    }

    fn provide(&mut self, _path: &[&Self::Fragment]) -> Vec<Self::Fragment> {
        todo!()
    }
}

impl ProviderExt for Proc {}

impl FilterSorter<ProcNode> for Proc {
    fn compare(&self, _a: &ProcNode, _b: &ProcNode) -> Option<Ordering> {
        todo!()
    }

    fn keep(&self, _a: &ProcNode) -> bool {
        todo!()
    }
}

impl Proc {
    pub fn new(_: &str) -> Result<Self> {
        Ok(Proc)
    }
}
