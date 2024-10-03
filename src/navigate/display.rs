use std::fmt::{Display, Formatter, Result as FmtResult};
use std::ops::Range;

use crate::tree::{NodeRef, Provider, ProviderExt};

use super::Navigate;

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

impl<P: Provider + ProviderExt> Display for Navigate<P>
where
    <P as Provider>::Fragment: Display,
{
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        write!(f, "\x1b[H\x1b[J")?;

        let mut view = self.view.borrow_mut();

        let visible = view.visible();
        view.line_mapping.resize(visible.len(), self.tree.root());

        let mut current = 0;
        self.fmt_at(
            f,
            self.tree.root(),
            "".into(),
            &mut current,
            &visible,
            &mut view.line_mapping,
        )?;
        view.total.end = current;

        if current < visible.end {
            write!(f, "{}", "\n".repeat(visible.end - current))?;
        }

        self.tree
            .provider()
            .fmt_frag_path(f, &self.tree.path_at(self.cursor))?;
        write!(f, "\r\n")?;

        if let Some(message) = &self.message {
            write!(f, "{message}    ")?;
            message.chars().count();
        }

        for k in &self.pending {
            if k.is_ascii_graphic() {
                write!(f, "{}", *k as char)
            } else {
                write!(f, "<{k}>")
            }?;
        }

        Ok(())
    }
}

impl<P: Provider> Navigate<P>
where
    <P as Provider>::Fragment: Display,
{
    fn fmt_at(
        &self,
        f: &mut Formatter,
        at: NodeRef,
        indent: String,
        current: &mut usize,
        visible: &Range<usize>,
        which: &mut [NodeRef],
    ) -> FmtResult {
        let node = self.tree.at(at);
        let frag = &node.fragment;

        if visible.contains(current) {
            if node.marked() {
                write!(f, " \x1b[4m")?;
            }
            if self.cursor == at {
                write!(f, "\x1b[7m")?;
            }
            write!(f, "{frag}\x1b[m")?;
            which[*current - visible.start] = at;
        }

        if node.folded() {
            if visible.contains(current) {
                write!(f, "\r\n")?;
            }
            *current += 1;
            return Ok(());
        }
        let children = node.children().unwrap();
        if 0 == children.len() {
            if visible.contains(current) {
                write!(f, "\r\n")?;
            }
            *current += 1;
            return Ok(());
        }

        if 1 == children.len() {
            self.fmt_at(f, children[0], indent, current, visible, which)
        } else {
            if visible.contains(current) {
                write!(f, "\r\n")?;
            }
            *current += 1;

            let mut iter = children.iter();
            let appearance = if self.options.pretty { PRETTY } else { ASCII };

            for it in iter.by_ref().take(children.len() - 1) {
                if visible.contains(current) {
                    write!(f, "{indent}{}", appearance.branch)?;
                }
                self.fmt_at(
                    f,
                    *it,
                    format!("{indent}{}", appearance.indent),
                    current,
                    visible,
                    which,
                )?;
            }

            if visible.contains(current) {
                write!(f, "{indent}{}", appearance.branch_last)?;
            }
            self.fmt_at(
                f,
                *iter.next().unwrap(),
                format!("{indent}{}", appearance.indent_last),
                current,
                visible,
                which,
            )
        }
    }
}
