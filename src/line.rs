use crate::{commands::Action, tree::Tree, view::View};
use crossterm::event::{Event, KeyCode};
use std::fs::Metadata;
use tui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Span, Spans},
    widgets::StatefulWidget,
};

pub enum Message {
    Info(String),
    Warning(String),
    Error(String),
}

pub struct Prompt {
    prompt: String,
    cursor: usize,
    content: String,
    action: Action,
    render_shift: usize,
}

#[derive(Default)]
pub struct Status {
    pending: Vec<char>,
    message: Option<Message>,
    input: Option<Prompt>,
    history: Vec<String>, //HashMap<String, String>, // TODO: hist per prompt (ie. not same for eg. ':' and '!')
    history_location: usize,
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
        if let Some(p) = &mut state.input {
            buf.set_spans(
                area.x + 2,
                area.y,
                &Spans::from({
                    let mut v = vec![Span::styled(
                        &*p.prompt,
                        Style::default().fg(Color::DarkGray),
                    )];

                    let avail = area.width as usize - 2 - p.prompt.len() - 2;

                    if p.content.len() < avail {
                        v.push(Span::raw(&p.content));
                    } else {
                        // update render_shift so as to keep cursor in view
                        if 0 < p.cursor && p.cursor < p.render_shift + 1 {
                            p.render_shift = p.cursor - 1;
                        }
                        if p.render_shift + avail - 1 < p.cursor {
                            p.render_shift = p.cursor - avail + 1;
                        }

                        let cut_start = if 0 < p.render_shift {
                            // need for ... at start
                            v.push(Span::raw("\u{2026}"));
                            p.render_shift + 1
                        } else {
                            // no need for ... at start
                            p.render_shift
                        };
                        let cut_end = cut_start + avail;

                        if p.content.len() < cut_start + avail {
                            // no need for ... at end
                            v.push(Span::raw(&p.content[cut_start..p.content.len()]));
                        } else {
                            // need for ... at end
                            v.push(Span::raw(&p.content[cut_start..cut_end - 1]));
                            v.push(Span::raw("\u{2026}"));
                        }
                    }
                    v
                }),
                area.width,
            );
        } else if let Some(m) = &state.message {
            let (text, style) = match m {
                Message::Info(text) => (text, Style::default()),
                Message::Warning(text) => (text, Style::default().fg(Color::Yellow)),
                Message::Error(text) => (text, Style::default().fg(Color::Red)),
            };
            buf.set_string(area.x, area.y, text, style);
        } else {
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
                    area.width - 1,
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
}

fn split_line_args(c: &String) -> Vec<String> {
    // TODO
    c.split(" ").map(String::from).collect()
}

impl Status {
    pub fn cursor_shift(&self) -> Option<u16> {
        self.input
            .as_ref()
            .map(|p| (2 + p.prompt.len() + p.cursor - p.render_shift as usize) as u16)
    }

    pub fn message(&mut self, message: Message) {
        self.message = Some(message);
    }

    pub fn prompt(&mut self, prompt: String, action: Action) {
        self.input = Some(Prompt {
            prompt,
            cursor: 0,
            content: String::new(),
            action,
            render_shift: 0,
        });
    }

    pub fn do_event(&mut self, event: &Event) -> (Option<(Action, Vec<String>)>, bool) {
        let Some(p) = &mut self.input else { return (None, false); };

        // YYY: maybe not right away, when is it stored in a registed is not thought out yet
        self.message = None;

        if let Event::Key(key) = event {
            match key.code {
                KeyCode::Char(c) => {
                    p.content.insert(p.cursor as usize, c);
                    p.cursor += 1;
                }
                KeyCode::Backspace => {
                    if 0 < p.cursor {
                        p.cursor -= 1;
                        p.content.remove(p.cursor as usize);
                    }
                }
                KeyCode::Delete => {
                    if p.cursor < p.content.len() {
                        p.content.remove(p.cursor as usize);
                    }
                }

                KeyCode::Left => {
                    if 0 < p.cursor {
                        p.cursor -= 1
                    }
                }
                KeyCode::Right => {
                    if p.cursor < p.content.len() {
                        p.cursor += 1
                    }
                }
                KeyCode::Home => p.cursor = 0,
                KeyCode::End => p.cursor = p.content.len(),

                KeyCode::Down | KeyCode::PageDown => {
                    if self.history_location + 1 < self.history.len() {
                        self.history_location += 1;
                        p.content.replace_range(
                            0..p.content.len(),
                            &self.history[self.history_location],
                        );
                        p.cursor = p.content.len();
                    }
                }
                KeyCode::Up | KeyCode::PageUp => {
                    if 0 < self.history_location {
                        self.history_location -= 1;
                        p.content.replace_range(
                            0..p.content.len(),
                            &self.history[self.history_location],
                        );
                        p.cursor = p.content.len();
                    }
                }

                KeyCode::Enter => {
                    let action = p.action.clone();
                    let args = split_line_args(&p.content);
                    self.history.push(p.content.clone());
                    self.history_location = self.history.len();
                    self.input = None;
                    return (Some((action, args)), true);
                }
                KeyCode::Esc => {
                    self.input = None;
                    return (None, true);
                }

                _ => (),
            }
        }

        return (None, true);
    }
}

impl Line<'_> {
    pub fn new<'app>((focused, tree): (&'app View, &'app Tree)) -> Line<'app> {
        Line { focused, tree }
    }
}
