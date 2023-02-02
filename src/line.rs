use crate::{tree::Tree, view::View};
use std::fs::Metadata;
use tui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::{Span, Spans},
    widgets::StatefulWidget,
};

#[derive(Default)]
pub struct Status {
    pending: Vec<char>,
    input: String,
}

impl Status {
    pub fn push_pending(&mut self, key: char) {
        self.pending.push(key);
    }
    pub fn get_pending(&self) -> &Vec<char> {
        &self.pending
    }
    pub fn clear_pending(&mut self) {
        self.pending.clear();
    }
}

pub struct Line<'me> {
    focused: &'me View,
    tree: &'me Tree,
}

fn perm_to_string(o: u32) -> String {
    [
        if o >> 2 & 0b1 == 1 { 'r' } else { '-' },
        if o >> 1 & 0b1 == 1 { 'w' } else { '-' },
        if o >> 0 & 0b1 == 1 { 'x' } else { '-' },
    ]
    .into_iter()
    .collect()
}

#[cfg(unix)]
fn meta_to_string(meta: &Metadata) -> String {
    use std::os::unix::fs::FileTypeExt;
    use std::os::unix::fs::PermissionsExt;

    let mode = meta.permissions().mode();
    let ft = meta.file_type();

    [
        // file type
        if ft.is_block_device() {
            'b'
        } else if ft.is_char_device() {
            'c'
        } else if ft.is_dir() {
            'd'
        } else if ft.is_symlink() {
            'l'
        } else if ft.is_fifo() {
            'p'
        } else if ft.is_socket() {
            's'
        } else {
            '-'
        }
        .to_string(),
        // owner
        perm_to_string(mode >> 3 * 2 & 0b111),
        // group
        perm_to_string(mode >> 3 * 1 & 0b111),
        // world
        perm_to_string(mode >> 3 * 0 & 0b111),
    ]
    .concat()
}

#[cfg(windows)]
fn meta_to_string(meta: &Metadata) -> String {
    use std::os::windows::fs::FileTypeExt;
    use std::os::windows::fs::PermissionsExt;

    let ro = meta.permissions().readonly();
    let ft = meta.file_type();

    [
        // file type
        if ft.is_dir() {
            'd'
        } else if ft.is_symlink() {
            'l'
        } else {
            '-'
        }
        .to_string(),
        // owner
        perm_to_string(0b101 | if ro { 0b000 } else { 0b010 }),
        // group
        perm_to_string(0b101 | if ro { 0b000 } else { 0b010 }),
        // world
        perm_to_string(0b101 | if ro { 0b000 } else { 0b010 }),
    ]
    .concat()
}

impl StatefulWidget for Line<'_> {
    type State = Status;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Status) {
        {
            let (node, _) = self.focused.at_cursor_pair(self.tree);
            buf.set_spans(
                area.x + 1,
                area.y,
                &Spans::from(vec![
                    Span::raw(match &node.meta {
                        Some(meta) => meta_to_string(meta),
                        None => "- no meta - ".to_string(),
                    }),
                    Span::raw(" "),
                    Span::styled(node.file_name(), node.style()),
                    Span::raw(node.decoration()),
                ]),
                area.width,
            );
        }

        if !state.pending.is_empty() {
            buf.set_string(
                area.x + area.width - state.pending.len() as u16 - 1,
                area.y,
                state.pending.iter().collect::<String>(),
                Style::default(),
            );
        }
    }
}

impl Line<'_> {
    pub fn new<'app>((focused, tree): (&'app View, &'app Tree)) -> Line<'app> {
        Line { focused, tree }
    }
}
