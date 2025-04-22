use std::cmp::Ordering;

use anyhow::Result;

use crate::navigate::Bidoof;
use crate::tree::{Fragment, Node, NodePath};

#[derive(Debug)]
pub struct Event<'a> {
    pub path: Vec<&'a Node>,
    pub kind: EventKind<'a>,
}
#[derive(Debug)]
pub enum EventKind<'a> {
    /// create (brand new node, folded and not loaded)
    Create(Fragment),
    /// modify (only the node itself, stays loaded if it was, no re-loading, may have a from-to)
    Modify(Option<Vec<&'a Node>>, Option<Fragment>),
    /// copies (deep copy, would be same -but not quite- as each remove + create)
    Copies(Option<Vec<&'a Node>>, Option<Fragment>),
    /// remove
    Remove,
    /// reload (remove for each contained, re-load may be lazy
    ///         ie same -but not quite- as remove + create + loading if it was
    ///         ie same -but not quite- as remove each + create each)
    Reload,
}
pub type EventPoller = Box<dyn for<'a> Fn(Bidoof<'a>) -> Event<'a>>;

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
    /// badibooda, you needee tha;
    /// the Vec part might just be `path.iter_all().collect()` every time
    fn split<'a>(&self, path: &'a NodePath<'a>, text: String) -> Result<(Vec<&'a Node>, Fragment)>;

    /// the default impl calls `display` for each components and joins with ""
    /// may be overridden to have additional text and sgr styling
    fn breadcrumbs(&self, path: &NodePath) -> String {
        let r: String = (0..path.head.len())
            .map(|k| self.display(&path.head[..=k].into()))
            .collect();
        r + &self.display(path)
    }

    /// returns a blocking function (TODO: but with timeout)
    /// `event_occured` can still be called when this returned `None`
    fn event_poller(&mut self) -> Option<EventPoller> {
        None
    }

    /// is notified of an event its event poller emitted
    /// event or will also be caused by a request
    /// mainly as an occasion to update said poller
    /// and actual structure/file/..
    fn event_occured(&mut self, event: &Event) {
        let name = std::any::type_name::<Self>();
        let path = self.components(&event.path[..].into());
        match &event.kind {
            EventKind::Create(frag) => crate::log!("{name}: create {path:?} {frag:?}"),
            EventKind::Modify(dest, frag) => {
                crate::log!(
                    "{name}: modify {path:?} {:?} {frag:?}",
                    dest.as_ref()
                        .map(|dest| self.components(&dest[..].into()))
                        .unwrap_or_default(),
                )
            }
            EventKind::Copies(dest, frag) => {
                crate::log!(
                    "{name}: copies {path:?} {:?} {frag:?}",
                    dest.as_ref()
                        .map(|dest| self.components(&dest[..].into()))
                        .unwrap_or_default(),
                )
            }
            EventKind::Remove => crate::log!("{name}: remove {path:?}"),
            EventKind::Reload => crate::log!("{name}: reload {path:?}"),
        }
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
    scratch::Scratch,
}
// lua::structs::ProviderFlags is updated manually...
