use std::cmp::Ordering;

use anyhow::Result;

use crate::tree::{Fragment, NodePath};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {}
pub type EventPoller = Box<dyn Fn() -> Event>;

/// A type that is able to provide a tree structure.
pub trait Provider: Send {
    fn provide_root(&self) -> Fragment;
    fn provide(&mut self, path: &NodePath) -> Vec<Fragment>;

    fn order(&self, left: &NodePath, right: &NodePath) -> Ordering;
    fn keep(&self, path: &NodePath) -> bool;

    /// display of a single node at path
    /// may be sgr stylized and decorated
    fn display(&self, path: &NodePath) -> String;
    /// each must be plain and undecorated
    fn components(&self, path: &NodePath) -> Vec<String>;
    /// essentially an undecorated, non sgr stylized, no additional text version of breadcrumbs
    fn join(&self, components: &[String]) -> String;

    /// the default impl calls `display` for each components and joins with ""
    /// may be overridden to have additional text and sgr styling
    fn breadcrumbs(&self, path: &NodePath) -> String {
        let r: String = (0..path.head.len())
            .map(|k| self.display(&path.head[..=k].into()))
            .collect();
        r + &self.display(path)
    }

    /// returns a blocking function
    fn event_poller(&mut self) -> Option<EventPoller> {
        None
    }
    /// is notified of an event its event poller emitted
    /// event may also be caused by a request
    /// mainly as an occasion to update said poller
    fn event_occured(&mut self, ev: &Event) {
        _ = ev;
    }

    /// Request to create a new node at `path`.
    /// `text` comes from user input and its interpretation is provider-dependent.
    fn request_mk(&mut self, path: &NodePath, text: String) -> Result<Option<Event>> {
        _ = (path, text);
        Ok(None)
    }
    /// Request to copy an existing node at `path`.
    /// `text` comes from user input and its interpretation is provider-dependent.
    fn request_cp(&mut self, path: &NodePath, text: String) -> Result<Option<Event>> {
        _ = (path, text);
        Ok(None)
    }
    /// Request to remove an existing node at `path`.
    fn request_rm(&mut self, path: &NodePath) -> Result<Option<Event>> {
        _ = (path,);
        Ok(None)
    }
    /// Request to move an existing node at `path`.
    /// `text` comes from user input and its interpretation is provider-dependent.
    /// The default implementation uses `request_cp` and `request_rm` which might be undesirable.
    fn request_mv(&mut self, path: &NodePath, text: String) -> Result<Option<Event>> {
        self.request_cp(path, text)?;
        self.request_rm(path)?;
        Ok(None)
    }
    /// Request to "change" an existing node at `path`.
    /// The meaning is provider- and input- dependent.
    /// `text` comes from user input and its interpretation is provider-dependent.
    fn request_ch(&mut self, path: &NodePath, text: String) -> Result<Option<Event>> {
        _ = (path, text);
        Ok(None)
    }
    /// Request to visualise more content / information about an existing node at `path`.
    fn request_vi(&mut self, path: &NodePath) -> Result<Vec<String>> {
        _ = (path,);
        Ok(Vec::new())
    }
    /// Arbitrary provider request extension. `path` may or may not be relevant.
    /// `text` comes from user input and its interpretation is provider-dependent.
    fn request_ex(
        &mut self,
        path: &NodePath,
        text: String,
    ) -> Result<(Vec<String>, Option<Event>)> {
        _ = (path, text);
        Ok((Vec::new(), None))
    }
}

macro_rules! providers {
    ($($nm:ident::$ty:ident),+$(,)?) => {
        $(pub mod $nm;)+

        pub const NAMES: &'static [&'static str] = &[$(stringify!($nm),)+];

        pub fn guess(arg: &str) -> Option<&'static str> {
            $(if $nm::$ty::guess(arg) {
                return Some(stringify!($nm));
            })+
            None
        }

        pub fn select(arg: &str, name: &str) -> Result<Box<dyn Provider>> {
            match name {
                $(stringify!($nm) => $nm::$ty::new(arg).map(|p| {
                    let p: Box<dyn Provider> = Box::new(p);
                    p
                }),)+
                _ => unreachable!(),
            }
        }
    }
}

providers! {
    fs::Fs,
}
// lua::structs::ProviderFlags is updated manually...
