use crate::{commands::Action, tree::Tree, view::View};
use crossterm::event::{Event, KeyCode, KeyModifiers};
use std::fs::Metadata;
use tui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Span, Spans},
    widgets::StatefulWidget,
};

fn str_char_slice(c: &str, a: usize, b: usize) -> &str {
    let mut i = c.char_indices();
    if let Some(st) = i.nth(a) {
        &c[st.0..i.nth(b - a - 1).unwrap_or((c.len(), ' ')).0]
    } else {
        ""
    }
}

pub enum Message {
    Info(String),
    Warning(String),
    Error(String),
}

// TODO: completion (would be provided by the `Action`)
pub struct Prompt {
    prompt: String,
    cursor: usize,
    content: String,
    action: Action,
    render_shift: usize,
    kill_ring: String,
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

                    let avail = area.width as usize - 2 - p.prompt.chars().count() - 2;

                    if p.content.chars().count() < avail {
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

                        if p.content.chars().count() < cut_start + avail {
                            // no need for ... at end
                            let char_cut_start = p.content.char_indices().nth(cut_start).unwrap().0;
                            v.push(Span::raw(&p.content[char_cut_start..]));
                        } else {
                            // need for ... at end
                            v.push(Span::raw(str_char_slice(
                                &p.content,
                                cut_start,
                                cut_end - 1,
                            )));
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

pub fn split_line_args(c: &str) -> Vec<String> {
    let mut r = Vec::new();
    let mut cur = String::new();

    let mut in_simple = false;
    let mut in_double = false;
    for ch in c.chars() {
        if in_simple {
            if '\'' == ch {
                in_simple = false;
            } else {
                cur.push(ch);
            }
        } else if in_double {
            if '"' == ch {
                in_double = false;
            } else {
                cur.push(ch);
            }
        } else {
            match ch {
                '\'' => in_simple = true,
                '"' => in_double = true,
                ' ' | '\t' => {
                    r.push(cur);
                    cur = String::new();
                }
                '#' if cur.is_empty() => break,
                _ => cur.push(ch),
            }
        }
    }

    if !cur.is_empty() {
        r.push(cur);
    }

    r
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
        let content = if self.history.len() == self.history_location {
            String::new()
        } else {
            self.history[self.history_location].clone()
        };
        self.input = Some(Prompt {
            prompt,
            cursor: if content.is_empty() {
                0
            } else {
                content.chars().count()
            },
            content,
            action,
            render_shift: 0,
            kill_ring: String::new(),
        });
    }

    pub fn do_event(&mut self, event: &Event) -> (Option<(Action, Vec<String>)>, bool) {
        let Some(p) = &mut self.input else { return (None, false); };

        // YYY: maybe not right away, when is it stored in a registed is not thought out yet
        self.message = None;

        if let Event::Key(key) = event {
            let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
            let alt = key.modifiers.contains(KeyModifiers::ALT);
            let both = ctrl && alt;
            let either = ctrl || alt;

            let replace = |p: &mut Prompt, niw: &str| {
                p.content.replace_range(.., niw);
                p.cursor = p.content.chars().count();
            };

            let hist_next =
                |p: &mut Prompt, history: &Vec<String>, history_location: &mut usize| {
                    if *history_location + 1 < history.len() {
                        *history_location += 1;
                        replace(p, &history[*history_location]);
                    }
                };
            let hist_prev =
                |p: &mut Prompt, history: &Vec<String>, history_location: &mut usize| {
                    if 0 < *history_location {
                        *history_location -= 1;
                        replace(p, &history[*history_location]);
                    }
                };

            match key.code {
                KeyCode::Char(c) => {
                    if both {
                        match c {
                            'b' => backward(p, TO_SHELL_WORD_BACKWARD),
                            'd' => kill(p, TO_SHELL_WORD_FORWARD),
                            'f' => forward(p, TO_SHELL_WORD_FORWARD),
                            'h' => kill(p, TO_SHELL_WORD_BACKWARD),
                            _ => (),
                        }
                    } else if ctrl {
                        match c {
                            'a' => p.cursor = 0,
                            'b' => backward(p, TO_CHAR_BACKWARD),
                            'c' | 'g' => {
                                // abord
                                self.input = None;
                                return (None, true);
                            }
                            'd' => kill(p, TO_CHAR_FORWARD),
                            'e' => p.cursor = p.content.chars().count(),
                            'f' => forward(p, TO_CHAR_FORWARD),
                            'h' => kill(p, TO_CHAR_BACKWARD),
                            'j' | 'm' => {
                                // accept
                                let action = p.action.clone();
                                let args = split_line_args(&p.content);
                                self.history.push(p.content.clone());
                                self.history_location = self.history.len();
                                self.input = None;
                                return (Some((action, args)), true);
                            }
                            'k' => kill(p, TO_END_FORWARD),
                            'n' => hist_next(p, &self.history, &mut self.history_location),
                            'o' => {
                                // accept - keep hist location
                                let action = p.action.clone();
                                let args = split_line_args(&p.content);
                                self.history.push(p.content.clone());
                                self.history_location += 1;
                                self.input = None;
                                return (Some((action, args)), true);
                            }
                            'p' => hist_prev(p, &self.history, &mut self.history_location),
                            'r' => panic!("i dont know whether to have bash' or vim's C-r (search hist or insert reg?)"),
                            't' => trans(p, TO_CHAR_BACKWARD, TO_CHAR_FORWARD),
                            'u' => kill(p, TO_END_BACKWARD),
                            'w' => kill(p, TO_SHELL_WORD_BACKWARD),
                            'y' => {
                                p.content.insert_str(p.cursor, &p.kill_ring);
                                p.cursor += p.kill_ring.len();
                            }
                            _ => (),
                        }
                    } else if alt {
                        match c {
                            'b' => backward(p, TO_WORD_BACKWARD),
                            'c' => {
                                // capitalize
                                // this one not look good but gave up
                                let (a, b) = TO_EXACT_WORD_FORWARD(&p.content, p.cursor);
                                if a < b {
                                    let w = str_char_slice(&p.content, a, b);
                                    let (capital, lower) = {
                                        let mut i = w.chars();
                                        (
                                            i.next().unwrap().to_uppercase().to_string(),
                                            i.collect::<String>().to_lowercase(),
                                        )
                                    };
                                    p.content.replace_range(a..=a, &capital);
                                    p.content.replace_range(a + 1..b, &lower);
                                    p.cursor = b;
                                }
                            }
                            'd' => kill(p, TO_WORD_FORWARD),
                            'f' => forward(p, TO_WORD_FORWARD),
                            'l' => {
                                // lower
                                let p_cursor = p.cursor;
                                forward(p, TO_WORD_FORWARD);
                                let lower =
                                    str_char_slice(&p.content, p_cursor, p.cursor).to_lowercase();
                                p.content.replace_range(p_cursor..p.cursor, &lower);
                            }
                            't' => trans(p, TO_EXACT_WORD_BACKWARD, TO_EXACT_WORD_FORWARD),
                            'u' => {
                                // upper
                                let p_cursor = p.cursor;
                                forward(p, TO_WORD_FORWARD);
                                let upper =
                                    str_char_slice(&p.content, p_cursor, p.cursor).to_uppercase();
                                p.content.replace_range(p_cursor..p.cursor, &upper);
                            }
                            '<' => {
                                self.history_location = 0;
                                replace(p, &self.history[self.history_location]);
                            }
                            '>' => {
                                self.history_location = self.history.len();
                                replace(p, ""); // XXX: this should be restoring it, edits are losts
                            }
                            _ => (),
                        }
                    } else {
                        p.content.insert(p.cursor as usize, c);
                        p.cursor += 1;
                    }
                } // if ..(c) = key.code

                KeyCode::Backspace => {
                    if alt {
                        kill(p, TO_WORD_BACKWARD);
                    } else {
                        kill(p, TO_CHAR_BACKWARD);
                    }
                }

                KeyCode::Delete => {
                    if ctrl {
                        kill(p, TO_WORD_FORWARD);
                    } else {
                        kill(p, TO_CHAR_FORWARD);
                    }
                }

                KeyCode::Left => {
                    if either {
                        backward(p, TO_WORD_BACKWARD);
                    } else {
                        backward(p, TO_CHAR_BACKWARD);
                    }
                }
                KeyCode::Right => {
                    if either {
                        forward(p, TO_WORD_FORWARD);
                    } else {
                        forward(p, TO_CHAR_FORWARD);
                    }
                }
                KeyCode::Home => p.cursor = 0,
                KeyCode::End => p.cursor = p.content.chars().count(),

                KeyCode::Down | KeyCode::PageDown => {
                    hist_next(p, &self.history, &mut self.history_location)
                }
                KeyCode::Up | KeyCode::PageUp => {
                    hist_prev(p, &self.history, &mut self.history_location)
                }

                KeyCode::Enter => {
                    // accept
                    let action = p.action.clone();
                    let args = split_line_args(&p.content);
                    self.history.push(p.content.clone());
                    self.history_location = self.history.len();
                    self.input = None;
                    return (Some((action, args)), true);
                }
                KeyCode::Esc => {
                    // abord
                    self.input = None;
                    return (None, true);
                }

                _ => (),
            } // match key.code
        } // if ..(key) = event

        return (None, true);
    }
}

impl Line<'_> {
    pub fn new<'app>((focused, tree): (&'app View, &'app Tree)) -> Line<'app> {
        Line { focused, tree }
    }
}

type TextObject = fn(&str, usize) -> (usize, usize);

const TO_CHAR_FORWARD: TextObject = |t, p| if p < t.len() { (p, p + 1) } else { (p, p) };
const TO_CHAR_BACKWARD: TextObject = |_, p| if 0 < p { (p - 1, p) } else { (p, p) };

const TO_WORD_FORWARD: TextObject = |t, p| {
    t.chars()
        .enumerate()
        .skip(p)
        .skip_while(|(_, c)| !c.is_ascii_alphanumeric())
        .skip_while(|(_, c)| c.is_ascii_alphanumeric())
        .next()
        .map_or((p, t.len()), |(e, _)| (p, e))
};
const TO_WORD_BACKWARD: TextObject = |t, p| {
    t.chars()
        .enumerate()
        .take(p)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .skip_while(|(_, c)| !c.is_ascii_alphanumeric())
        .skip_while(|(_, c)| c.is_ascii_alphanumeric())
        .next()
        .map_or((0, p), |(b, _)| (b + 1, p))
};

const TO_EXACT_WORD_FORWARD: TextObject = |t, p| {
    let chs = t.chars().enumerate().skip(p);

    let (Some((beg, _)), chs) = ({
        let mut tmp = chs.skip_while(|(_, c)| !c.is_ascii_alphanumeric());
        (tmp.next(), tmp)
    }) else { return (p, t.len()) };

    let (Some((end, _)), _) = ({
        let mut tmp = chs.skip_while(|(_, c)| c.is_ascii_alphanumeric());
        (tmp.next(), tmp)
    }) else { return (beg, t.len()) };

    (beg, end)
};
const TO_EXACT_WORD_BACKWARD: TextObject = |t, p| {
    let chs = t
        .chars()
        .enumerate()
        .take(p)
        .collect::<Vec<_>>()
        .into_iter()
        .rev();

    let (Some((end, _)), chs) = ({
        let mut tmp = chs.skip_while(|(_, c)| !c.is_ascii_alphanumeric());
        (tmp.next(), tmp)
    }) else { return (0, p) };

    let (Some((beg, _)), _) = ({
        let mut tmp = chs.skip_while(|(_, c)| c.is_ascii_alphanumeric());
        (tmp.next(), tmp)
    }) else { return (0, end) };

    (beg, end)
};

const TO_SHELL_WORD_FORWARD: TextObject = |t, p| {
    t.chars()
        .enumerate()
        .skip(p)
        .skip_while(|(_, c)| c.is_ascii_whitespace())
        .skip_while(|(_, c)| !c.is_ascii_whitespace())
        .next()
        .map_or((p, t.len()), |(e, _)| (p, e))
};
const TO_SHELL_WORD_BACKWARD: TextObject = |t, p| {
    t.chars()
        .enumerate()
        .take(p)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .skip_while(|(_, c)| c.is_ascii_whitespace())
        .skip_while(|(_, c)| !c.is_ascii_whitespace())
        .next()
        .map_or((0, p), |(b, _)| (b + 1, p))
};

const TO_END_FORWARD: TextObject = |t, p| (p, t.len());
const TO_END_BACKWARD: TextObject = |_, p| (0, p);

fn forward(p: &mut Prompt, to: TextObject) {
    let (_, b) = to(&p.content, p.cursor);
    p.cursor = b;
}

fn backward(p: &mut Prompt, to: TextObject) {
    let (a, _) = to(&p.content, p.cursor);
    p.cursor = a;
}

fn kill(p: &mut Prompt, to: TextObject) {
    let (a, b) = to(&p.content, p.cursor);
    if a < p.cursor {
        p.kill_ring.insert_str(0, str_char_slice(&p.content, a, b));
    } else {
        p.kill_ring.push_str(str_char_slice(&p.content, a, b));
    }
    p.content.replace_range(a..b, "");
    p.cursor = a;
}

// XXX: transpose-chars crashes at edges and transpose-words is broken
fn trans(p: &mut Prompt, to_left: TextObject, to_right: TextObject) {
    let (la, lb) = to_left(&p.content, p.cursor);
    let (ra, rb) = to_right(&p.content, p.cursor);
    let rcpy = str_char_slice(&p.content, ra, rb).to_string();
    let lcpy = str_char_slice(&p.content, la, lb).to_string();
    p.content.replace_range(ra..rb, &lcpy);
    p.content.replace_range(la..lb, &rcpy);
    p.cursor = rb;
}
