use std::io::{self, Result as IoResult, Write};
use std::ops::{Deref, Range};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::Builder;

use anyhow::Result;
use thiserror::Error;

use crate::lua::structs::{IndexPath, NodeInfo, Target};
use crate::navigate::options::GlobalOptionsRef;
use crate::navigate::view::{View, ViewSpaceSubset};
use crate::provider::{Event, EventKind, Provider};
use crate::tree::{Fragment, Node};

pub struct Space {
    pub tree: Node,
    pub provider: Box<dyn Provider>,
    provider_name: String,
    pub view: View,
    cursor: (usize, IndexPath),
    pub options: GlobalOptionsRef,
}

/// give the EventPoller access to the root node
/// so it can build and return an event
pub struct Bidoof<'a>(&'a Mutex<Space>);
pub struct BidoofGuard<'a>(MutexGuard<'a, Space>);

impl Bidoof<'_> {
    pub fn root(&self) -> BidoofGuard<'_> {
        BidoofGuard(self.0.lock().unwrap())
    }
}

impl Deref for BidoofGuard<'_> {
    type Target = Node;

    fn deref(&self) -> &Self::Target {
        &self.0.tree
    }
}

impl Drop for Space {
    fn drop(&mut self) {
        // TODO: stop poller thread
    }
}

#[derive(Error, Debug)]
pub enum SpaceNavError {
    #[error("erroneous target")]
    BrokenTarget,
    #[error("attempt at removing root")]
    CannotRemoveRoot,
}

impl Space {
    fn new_without_poller_thread(
        provider: Box<dyn Provider>,
        provider_name: String,
        options: GlobalOptionsRef,
    ) -> Self {
        Self {
            tree: Node::new(provider.provide_root()),
            provider,
            provider_name,
            view: View::default(),
            cursor: (0, IndexPath::default()),
            options,
        }
    }

    pub fn new(
        provider: Box<dyn Provider>,
        provider_name: String,
        options: GlobalOptionsRef,
    ) -> Arc<Mutex<Self>> {
        let space = Self::new_without_poller_thread(provider, provider_name.clone(), options);
        let space = Arc::new(Mutex::new(space));

        let moved = space.clone();
        _ = Builder::new().name(provider_name).spawn(move || {
            crate::log!("spawned");
            let Some(poller) = moved.lock().unwrap().provider.event_poller() else {
                crate::log!("no poller");
                return;
            };
            loop {
                crate::log!("poller loop");
                // blocks
                let event = poller(Bidoof(&moved));
                crate::log!("got event {event:?}");
                // TODO: debounce if appears necessary

                if moved
                    .lock()
                    .ok()
                    .and_then(|mut space| {
                        space.process_event(event);

                        // TODO: for now will always redraw
                        let mut buf = b"\x1b7".to_vec();
                        space.view_render(&mut buf, false).unwrap(); // unwrap: render to a vec
                        buf.extend(b"\x1b8");
                        io::stderr().write_all(&buf).ok()
                    })
                    .is_none()
                {
                    // assume unrecoverable situation, bail out
                    crate::log!("something went wrong");
                    break;
                }
            }
            crate::log!("exiting");
        });

        space
    }

    // {{{ process event stuff (private thingy)
    fn process_event(&mut self, event: Event) {
        self.provider.event_occured(&event);
        let path = self.tree.unresolve(&event.path[..].into());
        use EventKind::*;
        match event.kind {
            Copies(dest, frag) => todo!("process_event_copies({path:?}, {dest:?}, {frag:?})"),
            Create(frag) => self.process_event_create(&path[..], frag),
            Modify(dest, frag) => todo!("process_event_modify({path:?}, {dest:?}, {frag:?})"),
            Reload => todo!("process_event_reload({path:?})"),
            Remove => self.process_event_remove(&path[..]),
        }
    }

    /// does not call `provider.event_occured`
    fn process_event_create(&mut self, path: &[usize], frag: Fragment) {
        let child = Node::new(frag);
        crate::log!("event: create {path:?} {child:#?}");
        self.tree.add_child(&mut *self.provider, path, child);
    }

    /// does not call `provider.event_occured`
    fn process_event_remove(&mut self, path: &[usize]) {
        crate::log!("event: remove {path:?}");
        let l = path.len() - 1;
        self.tree.remove_child(&path[..l], path[l]);
    }
    // }}}

    // {{{ request event stuff (public interface)
    /// `request_..` is the public interface to `process_..`;
    /// it calls the later as well as `provider.event_request`/`.._occured`
    pub fn request_event_create(&mut self, target: Target, text: String) -> Result<()> {
        let index_path = self
            .target_to_path(target)
            .ok_or(SpaceNavError::BrokenTarget)?;
        let (tree, p) = self.tree_and_mut_provider();

        let path = tree.resolve(&index_path);
        let node_path = &path[..].into();

        let (path, frag) = p.split(node_path, text)?;
        //if path.is_empty() {
        //    return Err("path for target ended up empty");
        //}

        let kind = EventKind::Create(frag);
        let event = Event { path, kind };
        p.event_request(&event);
        p.event_occured(&event);

        // might unresolve to the same exact one (if node_path and path are
        // same ie when split did just iter_all collect) tho also might not
        let index_path = tree.unresolve(node_path);
        // well n't this be dum
        let frag = match event.kind {
            EventKind::Create(frag) => frag,
            _ => unreachable!(),
        };
        self.process_event_create(&index_path, frag);
        Ok(())
    }

    /// `request_..` is the public interface to `process_..`;
    /// it calls the later as well as `provider.event_request`/`.._occured`
    pub fn request_event_remove(&mut self, target: Target) -> Result<()> {
        let index_path = self
            .target_to_path(target)
            .ok_or(SpaceNavError::BrokenTarget)?;
        if index_path.is_empty() {
            return Err(SpaceNavError::CannotRemoveRoot)?;
        }
        let (tree, p) = self.tree_and_mut_provider();

        let event = Event {
            path: tree.resolve(&index_path),
            kind: EventKind::Remove,
        };
        p.event_request(&event);
        p.event_occured(&event);

        self.process_event_remove(&index_path);
        Ok(())
    }
    // }}}
    // }}}

    /// Specific borrows needed in `treest:provider_request` because
    /// it cannot be analyzed by the borrow checker through the MutexGuard.
    pub fn tree_and_mut_provider(&mut self) -> (&Node, &mut dyn Provider) {
        (&self.tree, &mut *self.provider)
    }

    pub fn provider_name(&self) -> &str {
        &self.provider_name
    }

    /// Delegate to `View::update` for both borrow and privacy reasons.
    pub fn view_update(&mut self, cols: Range<usize>, rows: usize, has_multiple_spaces: bool) {
        self.view.update(cols, rows, has_multiple_spaces);
    }

    /// Delegate to `View::render` for both borrow and privacy reasons.
    pub fn view_render(&mut self, f: &mut impl Write, force: bool) -> IoResult<()> {
        let sub = ViewSpaceSubset {
            root: &self.tree,
            provider: &*self.provider,
            cursor: &self.cursor.1[..self.cursor.0],
        };
        self.view.render(f, force, sub, &self.options)
    }

    pub fn cursor(&self) -> &[usize] {
        &self.cursor.1[..self.cursor.0]
    }

    pub fn cursor_to(&mut self, tip: usize, index_path: IndexPath) {
        // TODO: check validity before assigning, unfolding and/or cropping as needed
        self.cursor = (tip, index_path);
    }

    pub fn cursor_head(&self) -> &[usize] {
        &self.cursor.1[..self.cursor.0 - 1]
    }

    pub fn cursor_tail_mut(&mut self) -> &mut usize {
        &mut self.cursor.1[self.cursor.0 - 1]
    }

    pub fn cursor_truncate(&mut self) {
        self.cursor.1.truncate(self.cursor.0);
    }

    pub fn is_cursor_root(&self) -> bool {
        0 == self.cursor.0
    }

    ///// The first (bool) argument to the closure is `true` when the path is "trusted".
    //fn map_at<R>(&self, at: &Target, f: impl FnOnce(bool, &[usize]) -> R) -> R {
    //    let (trust, path) = match &at {
    //        Target::Cursor => (true, &self.cursor.1[..self.cursor.0]),
    //        Target::Path(path) => (false, &path[..]),
    //        Target::TrustedPath(path) => (true, &path[..]),
    //    };
    //    f(trust, path)
    //}

    pub fn target_to_path(&self, at: Target) -> Option<IndexPath> {
        match at {
            Target::Cursor => Some(self.cursor.1[..self.cursor.0].into()),
            Target::Marked(n) => self.iter_marked().nth(n),
            Target::Path(path) => self.tree.try_resolve_node(&path).map(|_| path),
            Target::TrustedPath(path) => Some(path),
        }
    }

    pub fn resolve_node(&self, at: &Target) -> Option<&Node> {
        match at {
            Target::Cursor => Some(self.tree.resolve_node(&self.cursor.1[..self.cursor.0])),
            Target::Marked(n) => self
                .iter_marked()
                .nth(*n)
                .map(|path| self.tree.resolve_node(&path)),
            Target::Path(path) => self.tree.try_resolve_node(path),
            Target::TrustedPath(path) => Some(self.tree.resolve_node(path)),
        }
    }

    pub fn resolve_node_mut(&mut self, at: &Target) -> Option<&mut Node> {
        match at {
            Target::Cursor => Some(self.tree.resolve_node_mut(&self.cursor.1[..self.cursor.0])),
            Target::Marked(n) => {
                let mby = self.iter_marked().nth(*n);
                mby.map(|path| self.tree.resolve_node_mut(&path))
            }
            Target::Path(path) => self.tree.try_resolve_node_mut(path),
            Target::TrustedPath(path) => Some(self.tree.resolve_node_mut(path)),
        }
    }

    pub fn retrieve_node_info(&self, at: Target) -> Option<NodeInfo> {
        self.resolve_node(&at).map(|node| {
            let path = self
                .target_to_path(at)
                .expect("resolve_node_mut should have returned None");
            let node_path = self.tree.resolve(&path);
            NodeInfo {
                name: self.provider.display(&node_path[..].into()),
                components: self.provider.components(&node_path[..].into()),
                breadcrumbs: self.provider.breadcrumbs(&node_path[..].into()),
                child_count: node.is_loaded().then(|| node.child_count()),
                path,
            }
        })
    }

    /// Return the target node's child count if valid.
    pub fn set_folded(&mut self, at: Target, is: bool) -> Option<usize> {
        let node = self.resolve_node_mut(&at)?;
        if is || node.is_loaded() {
            node.set_folded(is);
            return Some(node.child_count());
        }
        // note: at this point the path can be trusted because resolve_node_mut above
        let path = match &at {
            Target::Cursor => &self.cursor.1[..self.cursor.0],
            Target::Marked(n) => &self
                .iter_marked()
                .nth(*n)
                .expect("resolve_node_mut should have returned None")[..],
            Target::Path(path) => &path[..],
            Target::TrustedPath(path) => &path[..],
        };
        Some(self.tree.load(
            &mut *self.provider,
            path,
            // note: already know it isn't loaded, skip the check
            true,
            is,
        ))
    }
    pub fn get_folded(&self, at: Target) -> Option<bool> {
        self.resolve_node(&at).map(Node::is_folded)
    }

    pub fn set_marked(&mut self, at: Target, is: bool) {
        let Some(node) = self.resolve_node_mut(&at) else {
            return;
        };
        node.set_marked(is);
    }
    pub fn get_marked(&self, at: Target) -> Option<bool> {
        self.resolve_node(&at).map(Node::is_marked)
    }

    /// depth-first, parent after children
    pub fn iter_marked(&self) -> impl Iterator<Item = IndexPath> + use<'_> {
        let mut path = Vec::new();
        let mut stack: Vec<(&Node, _)> = Vec::new();
        stack.push((&self.tree, self.tree.children().map(|chs| chs.enumerate())));

        std::iter::from_fn(move || loop {
            if let Some((cur, chs)) = stack.last_mut() {
                if let Some((k, ch)) = chs.as_mut().and_then(|chs| chs.next()) {
                    path.push(k);
                    stack.push((ch, ch.children().map(|chs| chs.enumerate())));
                } else {
                    let marked = cur.is_marked().then(|| path.clone().into());
                    path.pop();
                    stack.pop();
                    if marked.is_some() {
                        return marked;
                    }
                }
            } else {
                return None;
            }
        })
    }

    pub fn cursor_enter(&mut self) {
        let Some(child_count) = self.set_folded(Target::Cursor, false) else {
            return;
        };
        if 0 == child_count {
            return;
        }
        if child_count <= self.cursor.1.len() {
            self.cursor_truncate();
        }
        if self.cursor.1.len() == self.cursor.0 {
            self.cursor.1.push(0);
        }
        self.cursor.0 += 1;
    }

    pub fn cursor_leave(&mut self) {
        self.cursor.0 = self.cursor.0.saturating_sub(1);
    }

    pub fn cursor_next(&mut self, wrapping: bool) {
        if 0 == self.cursor.0 {
            return;
        }
        let m = self.tree.resolve_node(self.cursor_head()).child_count();
        if 0 == m {
            return;
        }
        let k = self.cursor_tail_mut();
        match wrapping {
            false if *k < m - 1 => *k += 1,
            true if *k == m - 1 => *k = 0,
            true => *k += 1,
            _ => (),
        }
        self.cursor_truncate();
    }

    pub fn cursor_prev(&mut self, wrapping: bool) {
        if 0 == self.cursor.0 {
            return;
        }
        let m = self.tree.resolve_node(self.cursor_head()).child_count();
        if 0 == m {
            return;
        }
        let k = self.cursor_tail_mut();
        match wrapping {
            false if 0 < *k => *k -= 1,
            true if 0 == *k => *k = m - 1,
            true => *k -= 1,
            _ => (),
        }
        self.cursor_truncate();
    }
}
