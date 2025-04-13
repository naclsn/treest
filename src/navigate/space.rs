use std::io::{self, Result as IoResult, Write};
use std::ops::Range;
use std::sync::{Arc, Mutex};
use std::thread::Builder;

use crate::lua::structs::{IndexPath, NodeInfo, Target};
use crate::navigate::options::GlobalOptionsRef;
use crate::navigate::view::{View, ViewSpaceSubset};
use crate::provider::{Event, EventKind, Notif, NotifKind, Provider};
use crate::tree::{Fragment, Node, NodePath};

pub struct Space {
    pub tree: Node,
    pub provider: Box<dyn Provider>,
    provider_name: String,
    pub view: View,
    pub cursor: (usize, IndexPath),
    pub options: GlobalOptionsRef,
}

impl Drop for Space {
    fn drop(&mut self) {
        // TODO: stop poller thread
    }
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
                let event = poller();
                crate::log!("got event {event:?}");
                // TODO: debounce if appears necessary
                if moved
                    .lock()
                    .ok()
                    .and_then(|mut space| space.process_event(event).ok())
                    .is_none()
                {
                    // assume unrecoverable situation, bail out
                    crate::log!("something when wrong");
                    break;
                }
            }
            crate::log!("exiting");
        });

        space
    }

    /// This runs in the polling thread.
    ///
    /// It circle backs the event to the provider, perform the actual
    /// update on the tree (lazily as possible) and re-render only if
    /// it cannot be sure it is not necessary.
    pub fn process_event(&mut self, event: Event) -> IoResult<()> {
        fn path_trans<'a>(
            tree: &'a Node,
            provider: &dyn Provider,
            path: &[Fragment],
            backing_head: &'a mut Vec<&'a Node>,
        ) -> Option<(&'a [&'a Node], &'a Node)> {
            path.iter()
                .try_fold(tree, |node, frag| {
                    backing_head.push(node);
                    node.children()?
                        .iter()
                        .copied()
                        .find(|node| provider.compare(node.fragment_any(), frag))
                })
                .map(|tail| (&backing_head[..], tail))
        }
        let mut a = Vec::new(); // backing for event path
        let mut b = Vec::new(); // backing for modify dest
        let mut maybe_dest = None;

        let path = {
            let Some((head, tail)) = path_trans(&self.tree, &*self.provider, &event.path, &mut a)
            else {
                crate::log!("event: can't translate (broken path?)");
                return Ok(());
            };
            NodePath { head, tail }
        };
        let notif = Notif {
            path: &path,
            kind: match &event.kind {
                EventKind::Create(frag) => NotifKind::Create(frag),
                EventKind::Modify(dest, frag) => NotifKind::Modify(
                    {
                        if let Some(path) = dest {
                            let Some((head, tail)) =
                                path_trans(&self.tree, &*self.provider, path, &mut b)
                            else {
                                crate::log!("event: can't translate (broken dest path?)");
                                return Ok(());
                            };
                            maybe_dest = Some(NodePath { head, tail })
                        }
                        maybe_dest.as_ref()
                    },
                    frag.as_ref(),
                ),
                EventKind::Remove => NotifKind::Remove,
                EventKind::Reload => NotifKind::Reload,
            },
        };

        crate::log!("event: notif {notif:?}");
        self.provider.event_occured(&event, &notif);

        // TODO: todo
        match event.kind {
            EventKind::Create(frag) => {
                crate::log!("event: create {path:?} {:?}", Node::new(frag));
            }
            EventKind::Modify(_, frag) => {
                if let Some(frag) = frag {
                    crate::log!("event: modify {path:?} {maybe_dest:?} Some({frag:p})");
                } else {
                    crate::log!("event: modify {path:?} {maybe_dest:?} None");
                }
            }
            EventKind::Remove => {
                crate::log!("event: remove {path:?}");
            }
            EventKind::Reload => {
                crate::log!("event: reload {path:?}");
            }
        };

        let mut buf = b"\x1b7".to_vec();
        self.view_render(&mut buf, false).unwrap(); // unwrap: render to a vec
        buf.extend(b"\x1b8");
        io::stderr().write_all(&buf)
    }

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

    pub fn target_to_path(&self, at: Target) -> IndexPath {
        match at {
            Target::Cursor => self.cursor.1[..self.cursor.0].into(),
            Target::Path(path) => path, // XXX: untrusted path escalate to index path...
            Target::TrustedPath(path) => path,
        }
    }

    pub fn resolve_node(&self, at: &Target) -> Option<&Node> {
        match at {
            Target::Cursor => Some(self.tree.resolve_node(&self.cursor.1[..self.cursor.0])),
            Target::Path(path) => self.tree.try_resolve_node(path),
            Target::TrustedPath(path) => Some(self.tree.resolve_node(path)),
        }
    }

    pub fn resolve_node_mut(&mut self, at: &Target) -> Option<&mut Node> {
        match at {
            Target::Cursor => Some(self.tree.resolve_node_mut(&self.cursor.1[..self.cursor.0])),
            Target::Path(path) => self.tree.try_resolve_node_mut(path),
            Target::TrustedPath(path) => Some(self.tree.resolve_node_mut(path)),
        }
    }

    pub fn retrieve_node_info(&self, at: Target) -> Option<NodeInfo> {
        self.resolve_node(&at).map(|node| {
            let path = self.target_to_path(at);
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
        Some(self.tree.load(
            &mut self.provider,
            // note: at this point the path can be trusted because resolve_node_mut above
            match &at {
                Target::Cursor => &self.cursor.1[..self.cursor.0],
                Target::Path(path) => &path[..],
                Target::TrustedPath(path) => &path[..],
            },
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

    pub fn cursor_enter(&mut self) {
        let Some(child_count) = self.set_folded(Target::Cursor, false) else {
            return;
        };
        if 0 == child_count {
            return;
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
