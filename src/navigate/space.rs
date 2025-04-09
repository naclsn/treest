use std::io::{self, Result as IoResult, Write};
use std::ops::Range;
use std::sync::{Arc, Mutex};
use std::thread;

use crate::lua::structs::{IndexPath, NodeInfo, Target};
use crate::navigate::options::Options;
use crate::navigate::view::{View, ViewSpaceSubset};
use crate::provider::{Event, Provider};
use crate::tree::Node;

pub struct Space {
    pub tree: Node,
    pub provider: Box<dyn Provider>,
    provider_name: String,
    pub view: View,
    pub cursor: (usize, IndexPath),
}

impl Space {
    pub fn new(provider: Box<dyn Provider>, provider_name: String) -> Self {
        Self {
            tree: Node::new(provider.provide_root()),
            provider,
            provider_name,
            view: View::default(),
            cursor: (0, IndexPath::default()),
        }
    }

    pub fn spin_up_poller_thread(self) -> Arc<Mutex<Self>> {
        let r = Arc::new(Mutex::new(self));
        let space = r.clone();
        thread::spawn(move || {
            let Some(poller) = space.lock().unwrap().provider.event_poller() else {
                return;
            };
            while {
                let ev = poller(); // blocks
                space
                    .lock()
                    .ok()
                    .and_then(|mut sp| sp.process_event(&ev).ok())
                    .is_some()
            } {}
        });
        r
    }

    /// This runs in the polling thread.
    ///
    /// It circle backs the event to the provider, perform the actual
    /// update on the tree (lazily as possible) and re-render only if
    /// it cannot be sure it is not necessary.
    fn process_event(&mut self, ev: &Event) -> IoResult<()> {
        self.provider.event_occured(&ev);

        let need_render = todo!("process_event({ev:?})"); // TODO: actual tree update

        if need_render {
            let mut buf = b"\x1b7".to_vec();
            self.view_render(&mut buf, false, todo!(), todo!(), todo!(), todo!())
                .unwrap(); // unwrap: render to a vec
            buf.extend(b"\x1b8");
            io::stderr().write_all(&buf)
        } else {
            Ok(())
        }
    }

    /// Specific borrows needed in `treest:provider_request` because
    /// it cannot be analyzed by the borrow checker through the MutexGuard.
    pub fn tree_and_mut_provider(&mut self) -> (&Node, &mut dyn Provider) {
        (&self.tree, &mut *self.provider)
    }

    pub fn provider_name(&self) -> &str {
        &self.provider_name
    }

    /// Delegate to `View::render` for both borrow and privacy reasons.
    pub fn view_render(
        &mut self,
        f: &mut impl Write,
        force: bool,
        cols: Range<usize>,
        rows: usize,
        has_multiple_spaces: bool,
        options: &Options,
    ) -> IoResult<()> {
        let sub = ViewSpaceSubset {
            root: &self.tree,
            provider: self.provider.as_ref(),
            cursor: &self.cursor.1[..self.cursor.0],
        };
        self.view
            .render(f, force, sub, cols, rows, has_multiple_spaces, options)
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
