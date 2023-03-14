use crate::{
    commands::{Action, Key},
    textblock::TextBlock,
    tree::Tree,
    view::View,
};
use crossterm::event::{Event, KeyCode, KeyModifiers};
use tui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Span, Spans},
    widgets::StatefulWidget,
};

fn str_char_slice(c: &str, a: usize, b: usize) -> &str {
    if b <= a {
        return "";
    }
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

pub struct Prompt {
    prompt: String,
    cursor: usize,
    content: String,
    action: Action,
    render_shift: usize,
    kill_ring: String,
    hints: Option<Vec<String>>,
}

#[derive(Default)]
pub struct Status {
    pending: Vec<Key>,
    message: Option<Message>,
    message_tb: Option<TextBlock>,
    input: Option<Prompt>,
    history: Vec<String>, //HashMap<String, String>, // TODO: hist per prompt (ie. not same for eg. ':' and '!')
    history_location: usize,
}

pub struct Line<'me> {
    focused: &'me View,
    tree: &'me Tree,
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

                    let put_hints_here_if_any = |v: &mut Vec<_>| {
                        if let Some(hints) = &p.hints {
                            v.push(Span::raw(" "));
                            for it in hints {
                                v.push(Span::styled(it, Style::default().fg(Color::DarkGray)));
                                if !it.ends_with(' ') {
                                    v.push(Span::raw(" "));
                                }
                            }
                        }
                    };

                    let avail = area.width as usize - 2 - p.prompt.chars().count() - 2;

                    if p.content.chars().count() < avail {
                        v.push(Span::raw(str_char_slice(&p.content, 0, p.cursor)));
                        put_hints_here_if_any(&mut v);
                        v.push(Span::raw(str_char_slice(
                            &p.content,
                            p.cursor,
                            p.content.len(),
                        )));
                    } else {
                        // update render_shift so as to keep cursor in view
                        if 0 == p.cursor {
                            p.render_shift = 0;
                        }
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

                        v.push(Span::raw(str_char_slice(&p.content, cut_start, p.cursor)));
                        put_hints_here_if_any(&mut v);
                        if p.content.chars().count() < cut_start + avail {
                            // no need for ... at end
                            v.push(Span::raw(str_char_slice(&p.content, p.cursor, cut_end)));
                        } else {
                            // need for ... at end
                            v.push(Span::raw(str_char_slice(&p.content, p.cursor, cut_end - 1)));
                            v.push(Span::raw("\u{2026}"));
                        }
                    }

                    v
                }),
                area.width - 2,
            );
        } else if let Some(m) = &state.message {
            let (text, style) = match m {
                Message::Info(text) => (text, Style::default()),
                Message::Warning(text) => (text, Style::default().fg(Color::Yellow)),
                Message::Error(text) => (text, Style::default().fg(Color::Red)),
            };

            let width = area.width as usize;
            state.message_tb = Some(TextBlock::wrapped(text, width, style));
        } else {
            {
                let (_, node) = self.focused.at_cursor_pair(self.tree);
                buf.set_spans(
                    area.x + 1,
                    area.y,
                    &Spans::from(vec![
                        Span::raw(node.meta_to_string()),
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
                    state
                        .pending
                        .iter()
                        .map(|k| k.to_string())
                        .collect::<String>(),
                    Style::default(),
                );
            }
        }
    }
}

pub fn split_line_args_cursor_indices(
    c: &str,
    lookup: &impl Fn(&str) -> Vec<String>,
    cursor: usize,
    add_phantom_arg_at_cursor_and_skip_lookup: bool, // when true, will add a fake empty argument on floaty cursor and not perform lookup interpolation
) -> (Vec<String>, usize, usize) {
    let mut r = Vec::new();
    let mut cur = String::new();

    let mut passed_cursor = false;
    let mut arg_idx = 0;
    let mut ch_idx = 0;
    let mut done_with_cursor_stuff = !add_phantom_arg_at_cursor_and_skip_lookup;

    let mut in_simple = false;
    let mut in_double = false;
    let mut in_escape = false;
    let mut in_lookup = false;
    let mut lookup_name = String::new();
    let mut last_k = 0;
    for (k, ch) in c.chars().enumerate() {
        last_k = k;
        if !done_with_cursor_stuff && k == cursor {
            arg_idx = r.len();
            ch_idx = cur.len();
            passed_cursor = true;
        }

        if in_escape {
            cur.push(ch);
            in_escape = false;
        } else if in_lookup {
            if '}' == ch {
                let subst = lookup(&lookup_name);
                if !subst.is_empty() {
                    if in_double {
                        // push all in cur with spaces
                        cur.push_str(&subst.join(" "));
                    } else {
                        // push first in cur, then all in separate (stay on last one)
                        let len = subst.len();
                        cur.push_str(&subst[0]);
                        if 1 < len {
                            r.push(cur);
                            r.extend_from_slice(&subst[1..len - 1]);
                            cur = subst[len - 1].clone();
                        }
                    }
                }
                lookup_name = String::new();
                in_lookup = false;
            } else {
                lookup_name.push(ch);
            }
        } else if in_simple {
            if '\'' == ch {
                in_simple = false;
            } else {
                cur.push(ch);
            }
        } else if in_double {
            match ch {
                '"' => in_double = false,
                '{' if !add_phantom_arg_at_cursor_and_skip_lookup => in_lookup = true,
                _ => cur.push(ch),
            }
        } else {
            match ch {
                '\'' => in_simple = true,
                '"' => in_double = true,
                '\\' => in_escape = true,
                '{' if !add_phantom_arg_at_cursor_and_skip_lookup => in_lookup = true,
                ' ' | '\t' | '\n' | '\r' => {
                    if !cur.is_empty() {
                        if !done_with_cursor_stuff && k == cursor {
                            arg_idx = r.len();
                            ch_idx = cur.len();
                            done_with_cursor_stuff = true;
                        }
                        r.push(cur);
                        if !done_with_cursor_stuff && passed_cursor {
                            done_with_cursor_stuff = true;
                        }
                        cur = String::new();
                    } else if !done_with_cursor_stuff && passed_cursor {
                        // the case where the cursor is not on a word, eg.:
                        // `somecommand somearg1 | somearg3`
                        // so we add a fake empty argument here
                        r.push("".to_string());
                        done_with_cursor_stuff = true;
                    }
                }
                '#' if cur.is_empty() => break,
                _ => cur.push(ch),
            }
        }
    }

    if !cur.is_empty() {
        if !done_with_cursor_stuff && last_k + 1 == cursor {
            arg_idx = r.len();
            ch_idx = cur.len();
            done_with_cursor_stuff = true;
        }
        r.push(cur);
    }

    if !done_with_cursor_stuff && !passed_cursor {
        arg_idx = r.len();
        // the case where the cursor is not on a word, eg.:
        // `somecommand somearg1 somearg2 |`
        // so we add a fake empty argument here
        r.push("".to_string());
    }

    (r, arg_idx, ch_idx)
}

pub fn split_line_args(c: &str, lookup: &impl Fn(&str) -> Vec<String>) -> Vec<String> {
    split_line_args_cursor_indices(c, lookup, 0, false).0
}

impl Status {
    pub fn push_pending(&mut self, key: Key) {
        self.pending.push(key);
    }

    pub fn get_pending(&self) -> &Vec<Key> {
        &self.pending
    }

    pub fn clear_pending(&mut self) {
        self.pending.clear();
    }

    pub fn cursor_shift(&self) -> Option<u16> {
        self.input
            .as_ref()
            .map(|p| (2 + p.prompt.len() + p.cursor - p.render_shift) as u16)
    }

    pub fn message(&mut self, message: Message) {
        self.message = Some(message);
    }

    pub fn long_message(&self) -> Option<&TextBlock> {
        self.message_tb.as_ref()
    }

    pub fn clear_message(&mut self) {
        self.message = None;
        self.message_tb = None;
    }

    pub fn prompt(&mut self, prompt: String, action: Action, initial: Option<&str>) {
        let content = if let Some(init) = initial {
            self.history_location = self.history.len();
            init.to_string()
        } else if self.history.len() == self.history_location {
            String::new()
        } else {
            self.history[self.history_location].clone()
        };
        self.input = Some(Prompt {
            prompt,
            cursor: content.chars().count(),
            content,
            action,
            render_shift: 0,
            kill_ring: String::new(),
            hints: None,
        });
    }

    pub fn do_event(
        &mut self,
        event: &Event,
        lookup: &impl Fn(&str) -> Vec<String>,
    ) -> (Option<(Action, Vec<String>)>, bool) {
        let Some(p) = &mut self.input else { return (None, false); };
        p.hints = None;

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
                                self.clear_message();
                                return (None, true);
                            }
                            'd' => kill(p, TO_CHAR_FORWARD),
                            'e' => p.cursor = p.content.chars().count(),
                            'f' => forward(p, TO_CHAR_FORWARD),
                            'h' => kill(p, TO_CHAR_BACKWARD),
                            'i' => complete(p, lookup),
                            'j' | 'm' => {
                                // accept
                                let action = p.action.clone();
                                let args = split_line_args(&p.content, lookup);
                                self.history.push(p.content.clone());
                                self.history_location = self.history.len();
                                self.input = None;
                                self.clear_message();
                                return (Some((action, args)), true);
                            }
                            'k' => kill(p, TO_END_FORWARD),
                            'n' => hist_next(p, &self.history, &mut self.history_location),
                            'o' => {
                                // accept - keep hist location
                                let action = p.action.clone();
                                let args = split_line_args(&p.content, lookup);
                                self.history.push(p.content.clone());
                                self.history_location += 1;
                                self.input = None;
                                self.clear_message();
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
                                replace(p, ""); // XXX: this should be restoring it, edits are losts (same with ^N)
                            }
                            _ => (),
                        }
                    } else {
                        p.content.insert(p.cursor, c);
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
                    hist_next(p, &self.history, &mut self.history_location);
                }
                KeyCode::Up | KeyCode::PageUp => {
                    hist_prev(p, &self.history, &mut self.history_location);
                }

                KeyCode::Tab => complete(p, lookup),

                KeyCode::Enter => {
                    // accept
                    let action = p.action.clone();
                    let args = split_line_args(&p.content, lookup);
                    self.history.push(p.content.clone());
                    self.history_location = self.history.len();
                    self.input = None;
                    self.clear_message();
                    return (Some((action, args)), true);
                }
                KeyCode::Esc => {
                    if alt {
                        complete(p, lookup);
                    } else {
                        // abord
                        self.input = None;
                        self.clear_message();
                        return (None, true);
                    }
                }

                _ => (),
            } // match key.code
        } // if ..(key) = event

        (None, true)
    }
}

impl Line<'_> {
    pub fn new<'app>(focused: &'app View, tree: &'app Tree) -> Line<'app> {
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
        .find(|(_, c)| !c.is_ascii_alphanumeric())
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
        .find(|(_, c)| !c.is_ascii_alphanumeric())
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
        .find(|(_, c)| c.is_ascii_whitespace())
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
        .find(|(_, c)| c.is_ascii_whitespace())
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

fn complete(p: &mut Prompt, lookup: &impl Fn(&str) -> Vec<String>) {
    let (args, arg_idx, ch_idx) =
        split_line_args_cursor_indices(&p.content, lookup, p.cursor, true);

    // is it in "{.}"? (does not account for single quotes)
    let mut in_obj = 0; // ie after a '{'
    let mut in_ppt = 0; // ie after a '.' after a '{' (in_obj stays true)
    let mut obj = String::new();
    let mut ppt = String::new();
    for (k, ch) in args[arg_idx].chars().enumerate().take(ch_idx) {
        match ch {
            '{' if 0 == in_obj => in_obj = k + 1,
            '.' if 0 != in_obj && 0 == in_ppt => in_ppt = k + 1,
            '}' if 0 != in_obj => {
                in_obj = 0;
                in_ppt = 0;
                obj.clear();
                ppt.clear();
            }
            _ if 0 != in_ppt => ppt.push(ch),
            _ if 0 != in_obj => obj.push(ch),
            _ => (),
        }
    }

    let res = if 0 != in_obj {
        if 0 != in_ppt {
            ["file_name", "extension"]
                .iter()
                .filter(|it| it.starts_with(&ppt))
                .map(|it| " ".repeat(in_ppt) + it)
                .collect()
        } else {
            ["root", "selection"]
                .iter()
                .filter(|it| it.starts_with(&obj))
                .map(|it| " ".repeat(in_obj) + it)
                .collect()
        }
    } else {
        p.action.get_comp(
            &args.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            arg_idx,
            ch_idx,
            lookup,
        )
    };

    match res.len() {
        0 => {
            eprint!("\x07"); // bell
            p.hints = None;
        }
        1 => {
            let rest = str_char_slice(&res[0], ch_idx, res[0].len());
            p.content.insert_str(p.cursor, rest);
            p.cursor += rest.len();
            p.hints = None;
        }
        _ => {
            let mut common: &str = &res[0];
            let mut found = true;
            for it in res.iter().skip(1) {
                let min = std::cmp::min(it.len(), common.len());
                if let Some(((k, a), b)) = common
                    .char_indices()
                    .zip(it.chars())
                    .skip(ch_idx)
                    .find(|((k, a), b)| a != b || min <= *k + 1)
                {
                    common = &common[..if a == b { k + a.len_utf8() } else { k }]
                } else {
                    found = false;
                    break;
                }
            }
            if found {
                let rest = str_char_slice(common, ch_idx, res[0].len());
                p.content.insert_str(p.cursor, rest);
                p.cursor += rest.len();
            }
            p.hints = Some(res.into_iter().map(|s| s.trim().to_string()).collect());
        }
    }
}

// tests for split_line_args_..
#[cfg(test)]
mod tests {
    use super::split_line_args as s;

    fn lu(name: &str) -> Vec<String> {
        vec![if name.is_empty() {
            "[file_path]".to_string()
        } else {
            format!("name:{name}")
        }
        .to_string()]
    }

    macro_rules! t {
        ($(($in:literal, $out:expr),)*) => {
            $(assert_eq!(s($in, &lu), $out);)*
        };
    }

    #[test]
    fn test_split_args() {
        assert!(s("", &lu).is_empty());
        assert!(s(" ", &lu).is_empty());

        t![
            (r#" a "#, ["a"]),
            (r#"a b c d"#, ["a", "b", "c", "d"]),
            (r#"a 'b c' d"#, ["a", "b c", "d"]),
            (r#"a "b c" d"#, ["a", "b c", "d"]),
            (r#"a {w} d"#, ["a", "name:w", "d"]),
            (r#"a '{w}' d"#, ["a", "{w}", "d"]),
            (r#"a "{w}" d"#, ["a", "name:w", "d"]),
            (r#"a \{w} d"#, ["a", "{w}", "d"]),
            (r#"a \n d"#, ["a", "n", "d"]),
            (r#"a '\n' d"#, ["a", "\\n", "d"]),
            (r#"a "\n" d"#, ["a", "\\n", "d"]),
            (
                r#"bind e expand shell_wait 'sh -c "$EDITOR {}"'"#,
                ["bind", "e", "expand", "shell_wait", "sh -c \"$EDITOR {}\""]
            ),
            (
                r#"bind a expand prompt_init '-> {} shell mv {}'"#,
                ["bind", "a", "expand", "prompt_init", "-> {} shell mv {}"]
            ),
        ];
    }
}
