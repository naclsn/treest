use std::fmt::{Display, Formatter, Result as FmtResult};
use std::ops::Range;

use crate::navigate::Navigate;
use crate::terminal;
use crate::tree::{Node, NodePath};

struct Appearance {
    branch: &'static str,
    indent: &'static str,
    branch_last: &'static str,
    indent_last: &'static str,
}

const ASCII: Appearance = Appearance {
    branch: "|-- ",
    indent: "|   ",
    branch_last: "`-- ",
    indent_last: "    ",
};
const PRETTY: Appearance = Appearance {
    branch: "\u{251c}\u{2500}\u{2500} ",
    indent: "\u{2502}   ",
    branch_last: "\u{2514}\u{2500}\u{2500} ",
    indent_last: "    ",
};

impl Display for Navigate {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        write!(f, "\x1b[H\x1b[J")?;

        let cursor = self.resolve_cursor();

        let mut view = self.view.borrow_mut();

        let visible = view.visible();
        view.line_mapping.resize_with(visible.len(), Vec::new);

        let mut current = 0;
        self.fmt_at(
            f,
            &mut vec![&self.tree],
            cursor,
            "".into(),
            &mut current,
            &visible,
            //&mut view.line_mapping,
        )?;
        view.total.end = current;

        if current < visible.end {
            write!(f, "{}", "\n".repeat(visible.end - current))?;
        }

        //self.
        //    provider
        //    .write_nav_path(f, &self.tree.path_at(self.cursor))?;
        write!(f, "\r\n")?;

        if let Some(message) = &self.message {
            write!(f, "{message}    ")?;
            message.chars().count();
        }

        write!(f, "{}", terminal::keyseqstr(self.input.get_pending()))?;

        Ok(())
    }
}

impl Navigate {
    fn fmt_at(
        &self,
        f: &mut Formatter,

        at: &mut Vec<&Node>, // NodePathBuf (?maybe)
        cursor: &Node,

        indent: String,
        current: &mut usize,
        visible: &Range<usize>,
        //line_mapping: &mut [Vec<usize>],
    ) -> FmtResult {
        let node = at.last().unwrap();
        //let frag = &node.fragment;

        if visible.contains(current) {
            if node.marked() {
                write!(f, " \x1b[4m")?;
            }
            if std::ptr::eq(cursor, *node) {
                write!(f, "\x1b[7m")?;
            }
            let frag = self.provider.display(NodePath {
                head: &at[..at.len() - 1],
                tail: at.last().unwrap(),
            });
            write!(f, "{frag}\x1b[m")?;
            //line_mapping[*current - visible.start] = at.clone();
        }

        if node.folded() {
            if visible.contains(current) {
                write!(f, "\r\n")?;
            }
            *current += 1;
            return Ok(());
        }
        let children: &Vec<Node> = node.children().unwrap();
        if 0 == children.len() {
            if visible.contains(current) {
                write!(f, "\r\n")?;
            }
            *current += 1;
            return Ok(());
        }

        if 1 == children.len() {
            at.push(&children[0]);
            let r = self.fmt_at(
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
            //let appearance = if self.options.pretty { PRETTY } else { ASCII };
            let appearance = PRETTY;

            for it in iter.by_ref().take(children.len() - 1) {
                if visible.contains(current) {
                    write!(f, "{indent}{}", appearance.branch)?;
                }
                at.push(it);
                self.fmt_at(
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
            let r = self.fmt_at(
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
