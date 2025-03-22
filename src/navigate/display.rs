use std::io::{Result as IoResult, Write};
use std::ops::Range;

use crate::navigate::{Message, Navigate};
use crate::terminal;
use crate::tree::Node;

struct Appearance {
    branch: &'static str,
    indent: &'static str,
    branch_last: &'static str,
    indent_last: &'static str,
    scroll_before: &'static str,
    scroll_topmost: &'static str,
    scroll_top: &'static str,
    scroll_bar: &'static str,
    scroll_bot: &'static str,
    scroll_botmost: &'static str,
    scroll_after: &'static str,
}

const ASCII: Appearance = Appearance {
    branch: "|-- ",
    indent: "|   ",
    branch_last: "`-- ",
    indent_last: "    ",
    scroll_before: ": ",
    scroll_topmost: "% ",
    scroll_top: "+ ",
    scroll_bar: "# ",
    scroll_bot: "+ ",
    scroll_botmost: "% ",
    scroll_after: ": ",
};
const PRETTY: Appearance = Appearance {
    branch: "\u{251c}\u{2500}\u{2500} ",
    indent: "\u{2502}   ",
    branch_last: "\u{2514}\u{2500}\u{2500} ",
    indent_last: "    ",
    scroll_before: "\u{2502} ",
    scroll_topmost: "\u{2503} ",
    scroll_top: "\u{257d} ",
    scroll_bar: "\u{2503} ",
    scroll_bot: "\u{257f} ",
    scroll_botmost: "\u{2503} ",
    scroll_after: "\u{2502} ",
};

pub const MESSAGE_WINDOW_HEIGHT: usize = 12;

impl Navigate {
    // TODO: (almost everywhere) check and trim at terminal width (visible) characters
    pub fn render(&mut self, f: &mut impl Write) -> IoResult<()> {
        write!(f, "\x1b[H\x1b[J")?;

        let cursor = self.tree.resolve_node(self.cursor());

        let visible = self.view.visible();
        self.view.line_mapping.resize_with(visible.len(), Vec::new);

        let mut current = 0;
        self.render_at(
            f,
            &mut vec![&self.tree],
            cursor,
            "".into(),
            &mut current,
            &visible,
            //&mut view.line_mapping, // TODO: re-enable mouse interactions
        )?;
        self.view.total.end = current;

        if current < visible.end {
            write!(f, "{}", "\n".repeat(visible.end - current))?;
        }

        if let Some(Message { lines, offset, .. }) = &self.message {
            let len = std::cmp::min(lines.len(), MESSAGE_WINDOW_HEIGHT);
            write!(f, "\x1b[{}A", len)?;

            let top = offset * MESSAGE_WINDOW_HEIGHT / lines.len();
            let bot = std::cmp::min(
                top + MESSAGE_WINDOW_HEIGHT * MESSAGE_WINDOW_HEIGHT / lines.len(),
                len - 1,
            );
            let appearance = match self.options.appearance.as_str() {
                "pretty" => PRETTY,
                _ => ASCII,
            };

            for (k, line) in lines[*offset..*offset + len].iter().enumerate() {
                let sb = if k < top {
                    appearance.scroll_before
                } else if top == k && 0 == top {
                    appearance.scroll_topmost
                } else if bot == k && MESSAGE_WINDOW_HEIGHT - 1 == bot {
                    appearance.scroll_botmost
                } else if top == k {
                    appearance.scroll_top
                } else if k < bot {
                    appearance.scroll_bar
                } else if bot == k {
                    appearance.scroll_bot
                } else {
                    appearance.scroll_after
                };

                write!(f, "{sb}{line}\r\n")?;
            }

            if MESSAGE_WINDOW_HEIGHT < lines.len() {
                write!(f, "-- More ({} lines) --\r\n", lines.len())?;
            } else {
                write!(f, "-- (End) --\r\n")?;
            }
        } else {
            let path = self.tree.resolve(self.cursor());
            write!(f, "{}\r\n", self.provider.breadcrumb(&path[..].into()))?;
            write!(f, "{}", terminal::keyseqstr(self.input.get_pending()))?;
        }

        Ok(())
    }
}

impl Navigate {
    fn render_at(
        &self,
        f: &mut impl Write,

        at: &mut Vec<&Node>, // NodePathBuf (?maybe)
        cursor: &Node,

        indent: String,
        current: &mut usize,
        visible: &Range<usize>,
        //line_mapping: &mut [Vec<usize>],
    ) -> IoResult<()> {
        let node = at.last().unwrap();
        //let frag = &node.fragment;

        if visible.contains(current) {
            if node.is_marked() {
                write!(f, " \x1b[4m")?;
            }
            if std::ptr::eq(cursor, *node) {
                write!(f, "\x1b[7m")?;
            }
            let frag = self.provider.display(&at[..].into());
            write!(f, "{frag}\x1b[m")?;
            //line_mapping[*current - visible.start] = at.clone();
        }

        if node.is_folded() {
            if visible.contains(current) {
                write!(f, "\r\n")?;
            }
            *current += 1;
            return Ok(());
        }
        let children = node.children().unwrap();
        if children.is_empty() {
            if visible.contains(current) {
                write!(f, "\r\n")?;
            }
            *current += 1;
            return Ok(());
        }

        if 1 == children.len() {
            at.push(children[0]);
            let r = self.render_at(
                f, at, cursor, indent, current, visible, /*line_mapping*/
            );
            at.pop();
            r
        } else {
            if visible.contains(current) {
                write!(f, "\r\n")?;
            }
            *current += 1;

            let mut iter = children.iter();
            let appearance = match self.options.appearance.as_str() {
                "pretty" => PRETTY,
                _ => ASCII,
            };

            for it in iter.by_ref().take(children.len() - 1) {
                if visible.contains(current) {
                    write!(f, "{indent}{}", appearance.branch)?;
                }
                at.push(it);
                self.render_at(
                    f,
                    at,
                    cursor,
                    format!("{indent}{}", appearance.indent),
                    current,
                    visible,
                    //line_mapping,
                )?;
                at.pop();
            }

            if visible.contains(current) {
                write!(f, "{indent}{}", appearance.branch_last)?;
            }
            at.push(iter.next().unwrap());
            let r = self.render_at(
                f,
                at,
                cursor,
                format!("{indent}{}", appearance.indent_last),
                current,
                visible,
                //line_mapping,
            );
            at.pop();
            r
        }
    }
}
