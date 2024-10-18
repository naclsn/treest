use anyhow::Result;

use crate::tree::Provider;

use super::Navigate;

pub struct Command<N: ?Sized> {
    pub name: &'static str,
    pub help: &'static str,
    pub execute: Box<dyn FnMut(&mut N) -> Result<String>>,
}

pub enum Crap {
    Nav,
    Prov,
}
