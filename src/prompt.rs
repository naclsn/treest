use std::io::Write;
use std::mem;

use crate::lua::structs::PromptSplitInfo;

/// Split `line` in a shell-line args manner:
/// * words are split on (unicode) "whitespace" characters
/// * backslash and single quotes preserve literal meaning
/// * single quotes cannot contain a single quote
/// * backslash in double quotes escape the next character:
///     `\t` -> tab (0x09)
///     `\n` -> newline (0x10)
///     `\r` -> carriage return (0x13)
///     `\e` -> escape (0x1b)
///     anything else is the character itself (so `\"` -> double quote ...)
///
/// No environment variable interpolation is performed!
///
/// There is always at least 1 part in the result's `parts`.
/// `point` is used to set `in_part` to the index of the part containing it.
pub fn shell_like_split(line: &str, point: usize) -> PromptSplitInfo {
    let mut parts = Vec::new();
    let mut curr = String::new();
    let mut in_part = 0;

    enum State {
        Word,
        Blank,
        SingleQuote,
        DoubleQuote,
    }
    use State::*;

    let word = !line.chars().next().is_some_and(char::is_whitespace);
    let mut state = if word { Word } else { Blank };

    let mut chars = line.chars().enumerate();
    while let Some((k, c)) = chars.next() {
        match state {
            Word | Blank if '\'' == c => state = SingleQuote,
            Word | Blank if '\"' == c => state = DoubleQuote,

            Word if '\\' == c => match chars.next() {
                Some((_, c)) => curr.push(c),
                None => break,
            },
            Word if c.is_whitespace() => {
                if 0 == in_part && point < k {
                    in_part = parts.len();
                }
                parts.push(mem::take(&mut curr));
                state = Blank;
            }
            Word => curr.push(c),

            Blank if !c.is_whitespace() => {
                curr.push(c);
                state = Word;
            }
            Blank => (),

            SingleQuote if '\'' == c => state = Word,
            SingleQuote => curr.push(c),

            DoubleQuote if '\"' == c => state = Word,
            DoubleQuote if '\\' == c => match chars.next() {
                Some((_, 't')) => curr.push('\t'),
                Some((_, 'n')) => curr.push('\n'),
                Some((_, 'r')) => curr.push('\r'),
                Some((_, 'e')) => curr.push('\x1b'),
                Some((_, c)) => curr.push(c),
                None => break,
            },
            DoubleQuote => curr.push(c),
        }
    }
    if !matches!(state, Blank) {
        parts.push(curr);
    }

    PromptSplitInfo { parts, in_part }
}

/// Split `line` in lua tokens.
/// The input might be incomplete (such as an unclosed string or comment). Strings and comments
/// will contains their delimiters and escape sequences are not processed. Runs of unexpected
/// characters are grouped into singular tokens.
///
/// If the input only consists of whitespaces, the result's `parts` will be empty
/// and as such `in_part` will be irrelevant. Otherwise `in_part` is the index
/// of the token containing `point`.
pub fn lua_tokens_split(line: &str, point: usize) -> PromptSplitInfo {
    let mut parts = Vec::new();
    let mut in_part = 0;

    let mut head = 0;
    while {
        head = line.len() - dbg!(line[head..].trim_start()).len();
        head < line.len()
    } {
        let ahead = match line[head..].as_bytes() {
            [b'-', b'-', b'[', b'[' | b'=', rest @ ..] | [b'[', b'[' | b'=', rest @ ..] => {
                // on first '=' or '[' if long bracket of level 0
                let st = line.len() - rest.len() - 1;
                if let Some(level) = line[st..].find('[') {
                    let ed = line[st + level + 1..] // just past the '[===['
                        .find(&format!("]{}]", &line[st..st + level]))
                        .unwrap_or(line.len() - st - level - 1);
                    ed + level + 2
                } else if line[head..].starts_with("--") {
                    let nl = line[head..].find('\n').unwrap_or(line.len() - 1 - head);
                    head + nl + 1
                } else {
                    line.len()
                }
            }
            [b'-', b'-', ..] => {
                let nl = line[head..].find('\n').unwrap_or(line.len() - 1 - head);
                head + nl + 1
            }

            [b'a', b'n', b'd', rest @ ..]
            | [b'b', b'r', b'e', b'a', b'k', rest @ ..]
            | [b'd', b'o', rest @ ..]
            | [b'e', b'l', b's', b'e', b'i', b'f', rest @ ..]
            | [b'e', b'l', b's', b'e', rest @ ..]
            | [b'e', b'n', b'd', rest @ ..]
            | [b'f', b'a', b'l', b's', b'e', rest @ ..]
            | [b'f', b'o', b'r', rest @ ..]
            | [b'f', b'u', b'n', b'c', b't', b'i', b'o', b'n', rest @ ..]
            | [b'i', b'f', rest @ ..]
            | [b'i', b'n', rest @ ..]
            | [b'l', b'o', b'c', b'a', b'l', rest @ ..]
            | [b'n', b'i', b'l', rest @ ..]
            | [b'n', b'o', b't', rest @ ..]
            | [b'o', b'r', rest @ ..]
            | [b'r', b'e', b'p', b'e', b'a', b't', rest @ ..]
            | [b'r', b'e', b't', b'u', b'r', b'n', rest @ ..]
            | [b't', b'h', b'e', b'n', rest @ ..]
            | [b't', b'r', b'u', b'e', rest @ ..]
            | [b'u', b'n', b't', b'i', b'l', rest @ ..]
            | [b'w', b'h', b'i', b'l', b'e', rest @ ..] => {
                let keyword = &line[head..line.len() - rest.len()];
                head + keyword.len()
            }

            // names and numerical constant
            [] => 0,

            [b'+', rest @ ..]
            | [b'-', rest @ ..]
            | [b'*', rest @ ..]
            | [b'/', rest @ ..]
            | [b'%', rest @ ..]
            | [b'^', rest @ ..]
            | [b'#', rest @ ..]
            | [b'=', b'=', rest @ ..]
            | [b'~', b'=', rest @ ..]
            | [b'<', b'=', rest @ ..]
            | [b'>', b'=', rest @ ..]
            | [b'<', rest @ ..]
            | [b'>', rest @ ..]
            | [b'=', rest @ ..]
            | [b'(', rest @ ..]
            | [b')', rest @ ..]
            | [b'{', rest @ ..]
            | [b'}', rest @ ..]
            | [b'[', rest @ ..]
            | [b']', rest @ ..]
            | [b';', rest @ ..]
            | [b':', rest @ ..]
            | [b',', rest @ ..]
            | [b'.', b'.', b'.', rest @ ..]
            | [b'.', b'.', rest @ ..]
            | [b'.', rest @ ..] => {
                let punct = &line[head..line.len() - rest.len()];
                head + punct.len()
            }

            _ => todo!("{:?}", &line[head..]),
        };

        parts.push(line[head..ahead].to_string());

        if (head..ahead).contains(&point) {
            in_part = parts.len() - 1;
        }
        head = ahead;
    }

    //let mut chars = line.chars().enumerate();
    //while let Some((k, c)) = chars.next() {
    //}

    PromptSplitInfo { parts, in_part }
}

pub fn prompt(
    ps: &str,
    input: impl IntoIterator<Item = u8>,
    mut output: impl Write,
    mut history: Vec<String>,
    complete: impl Fn(&str, usize) -> Vec<String>,
) -> Option<String> {
    write!(output, "{ps}").ok()?;

    let mut at = 0;
    // not a string but a vec of char so it can be indexed directly
    let mut s = Vec::new();

    let mut in_hist = history.len();
    history.push(String::new());

    let mut pend = Vec::new();
    let mut input = input.into_iter();
    while let Some(key) = input.next() {
        pend.push(key);
        match &pend[..] {
            b"\x1bb" if 0 < at => {
                let by = s[..at]
                    .windows(2)
                    .rev()
                    .position(|p: &[char]| !p[0].is_alphanumeric() && p[1].is_alphanumeric())
                    .map(|k| k + 1)
                    .unwrap_or(at);
                write!(output, "\x1b[{by}D").ok()?;
                at -= by;
            }
            b"\x1bd" if at < s.len() => {
                let by = s[at..]
                    .windows(2)
                    .position(|p| p[0].is_alphanumeric() && !p[1].is_alphanumeric())
                    .map(|k| k + 1)
                    .unwrap_or(s.len() - at);
                write!(output, "\x1b[{by}P").ok()?;
                s.drain(at..at + by);
            }
            b"\x1bf" if at < s.len() => {
                let by = s[at..]
                    .windows(2)
                    .position(|p| p[0].is_alphanumeric() && !p[1].is_alphanumeric())
                    .map(|k| k + 1)
                    .unwrap_or(s.len() - at);
                write!(output, "\x1b[{by}C").ok()?;
                at += by;
            }
            b"\x1b\x1b" => return None,
            b"\x1b\x7f" if 0 < at => {
                let by = s[..at]
                    .windows(2)
                    .rev()
                    .position(|p: &[char]| !p[0].is_alphanumeric() && p[1].is_alphanumeric())
                    .map(|k| k + 1)
                    .unwrap_or(at);
                write!(output, "\x1b[{by}D\x1b[{by}P").ok()?;
                s.drain(at - by..at);
                at -= by;
            }

            [0x01] | b"\x1b[H" if 0 < at => {
                write!(output, "\x1b[{at}D").ok()?;
                at = 0;
            }
            [0x02] | b"\x1b[D" if 0 < at => {
                write!(output, "\x08").ok()?;
                at -= 1;
            }
            [0x03] => return None,
            [0x04] | b"\x1b[3~" => {
                if at < s.len() {
                    s.remove(at);
                    write!(output, "\x1b[P").ok()?;
                } else if 0 == at && s.is_empty() {
                    return None;
                }
            }
            [0x05] | b"\x1b[F" if at < s.len() => {
                write!(output, "\x1b[{}C", s.len() - at).ok()?;
                at = s.len();
            }
            [0x06] | b"\x1b[C" if at < s.len() => {
                write!(output, "{}", s[at]).ok()?;
                at += 1;
            }
            [.., 0x07] => pend.clear(),
            [0x08 | 127] => {
                if 0 == at && s.is_empty() {
                    return None;
                }
                at -= 1;
                s.remove(at);
                write!(output, "\x08\x1b[P").ok()?;
            }
            [0x09] => {
                //let (args, in_arg) = split(&s, at);
                //let hints = complete(args.iter().map(String::as_str).collect(), in_arg);
                let hints = complete(&s.into_iter().collect::<String>(), at);
                todo!("completion hints: {hints:?}"); // TODO(!)
            }
            [0x0a | 0x0d] => return Some(s.into_iter().collect()),
            [0x0b] => {
                write!(output, "\x1b[{}P", s.len() - at).ok()?;
                s.truncate(at);
            }
            [0x0c] => {
                write!(output, "\x1b[G\x1b[K{ps}").ok()?;
                s.iter().try_for_each(|c| write!(output, "{c}")).ok()?;
                write!(output, "\x1b[{}D", s.len() - at).ok()?;
            }
            [0x0e] if in_hist < history.len() - 1 => {
                if !s.is_empty() {
                    write!(output, "\x1b[{at}D\x1b[{}P", s.len()).ok()?;
                }
                in_hist += 1;
                s = history[in_hist].chars().collect();
                s.iter().try_for_each(|c| write!(output, "{c}")).ok()?;
                at = s.len();
            }
            [0x0f] => {
                // TODO: prompt ^O
                return Some(s.into_iter().collect());
            }
            [0x10] if 0 < in_hist => {
                if !s.is_empty() {
                    write!(output, "\x1b[{at}D\x1b[{}P", s.len()).ok()?;
                }
                in_hist -= 1;
                s = history[in_hist].chars().collect();
                s.iter().try_for_each(|c| write!(output, "{c}")).ok()?;
                at = s.len();
            }
            [0x15] => {
                write!(output, "\x1b[{at}D\x1b[{at}P").ok()?;
                s.drain(..at);
            }
            [0x17] if 0 < at => {
                let by = s[..at]
                    .windows(2)
                    .rev()
                    .position(|p: &[char]| p[0].is_whitespace() && !p[1].is_whitespace())
                    .map(|k| k + 1)
                    .unwrap_or(at);
                write!(output, "\x1b[{by}D\x1b[{by}P").ok()?;
                s.drain(at - by..at);
                at -= by;
            }

            b"\x1b" | b"\x1b[" | [0x1b, b'[', b'0'..=b'9'] => continue,
            [0x1b, ..] => (),

            [b' '..=255] => {
                let u = key as u32;
                let c = char::from_u32(match key {
                    0b11000000..=0b11011111 => {
                        let x = input.next()? as u32;
                        ((u & 31) << 6) | (x & 63)
                    }
                    0b11100000..=0b11101111 => {
                        let (x, y) = (input.next()? as u32, input.next()? as u32);
                        ((u & 15) << 12) | ((x & 63) << 6) | (y & 63)
                    }
                    0b11110000..=0b11110111 => {
                        let (x, y, z) = (
                            input.next()? as u32,
                            input.next()? as u32,
                            input.next()? as u32,
                        );
                        ((u & 7) << 18) | ((x & 63) << 12) | ((y & 63) << 6) | (z & 63)
                    }
                    _ => u,
                })?;
                write!(output, "\x1b[@{c}").ok()?;
                s.insert(at, c);
                at += 1;
            }

            _ => (),
        }
        pend.clear();
    }

    None
}

#[cfg(test)]
macro_rules! assert_parts {
    ($split:ident, $line:literal, $parts:expr $(,)?) => {
        let parts = $split($line, 0).parts;
        assert_eq!(parts, $parts, "{}({:?})", stringify!($split), $line);
    };
}

#[cfg(test)]
#[test]
fn test_split() {
    assert_parts!(shell_like_split, "", [""]);
    assert_parts!(shell_like_split, "coucou ", ["coucou"]);
    assert_parts!(
        shell_like_split,
        " this\tis\ntest    text",
        ["this", "is", "test", "text"],
    );
    assert_parts!(
        shell_like_split,
        r#" 'quo'ted and dis"joi'\n"'\t"' ye"\""y   "#,
        ["quoted", "and", "disjoi'\n\\t\"", "ye\"y"],
    );
    assert_parts!(shell_like_split, "it's fine", ["its fine"]);

    //assert_parts!(
    //    lua_tokens_split,
    //    "function len(t) return t.n or #t end", [""]);

    assert_parts!(
        lua_tokens_split,
        r#"
 -- hello
--[[
hi]]
--[===[ uuu ]] ]==] ]===]
        "#,
        [""]
    );
}
