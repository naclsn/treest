use std::io::Write;
use std::mem;

use crate::lua::structs::PromptSplitInfo;

/// Split `line` in a shell-line.
///
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
pub fn shell_like_split(line: &str, point: Option<usize>) -> PromptSplitInfo {
    let mut parts = Vec::new();
    let mut curr = String::new();
    let mut in_part = None;

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
        if in_part.is_none() && point.is_some_and(|p| p == k) {
            in_part = Some(parts.len());
            if matches!(state, Blank) && c.is_whitespace() {
                parts.push("".to_string());
            }
        }

        match state {
            Word | Blank if '\'' == c => state = SingleQuote,
            Word | Blank if '\"' == c => state = DoubleQuote,

            Word if '\\' == c => match chars.next() {
                Some((_, c)) => curr.push(c),
                None => break,
            },
            Word if c.is_whitespace() => {
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

    if !matches!(state, Blank) || parts.is_empty() || point.is_some() && in_part.is_none() {
        parts.push(curr);
    }
    if point.is_some() && in_part.is_none() {
        in_part = Some(parts.len() - 1);
    }

    PromptSplitInfo { parts, in_part }
}

/// Split `line` into lua tokens.
///
/// The input might be incomplete (such as an unclosed string or comment). Strings and comments
/// will contains their delimiters and escape sequences are not processed. Runs of unexpected
/// characters are grouped into singular tokens.
///
/// If the input only consists of whitespaces, the result's `parts` will be empty
/// and as such `in_part` will be irrelevant. Otherwise `in_part` is the index
/// of the token containing `point`.
pub fn lua_tokens_split(line: &str, point: Option<usize>) -> PromptSplitInfo {
    let mut parts = Vec::new();
    let mut in_part = None;

    let mut head = 0;
    while {
        head = line.len() - line[head..].trim_start().len();
        head < line.len()
    } {
        let ahead = match line[head..].as_bytes() {
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

            [b'-', b'-', b'[', b'[' | b'=', rest @ ..] | [b'[', b'[' | b'=', rest @ ..] => {
                // on first '=' or '[' if long bracket of level 0
                let st = line.len() - rest.len() - 1;
                let level = line[st..].chars().take_while(|c| '=' == *c).count();
                if st + level < line.len() && b'[' == line.as_bytes()[st + level] {
                    let ed = line[st + level + 1..] // slice is just past the '[===['
                        .find(&format!("]{}]", &line[st..st + level]))
                        .unwrap_or(line.len() - st - level - 1);
                    st + level + 1 + ed + level + 2
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

            [b'0'..=b'9', ..] | [b'.', b'0'..=b'9', ..] => {
                enum State {
                    Integral,
                    Fractional,
                    ExponentSign,
                    Exponent,
                    Hexadecimal,
                }
                use State::*;

                let mut state = Integral;
                let ox = line[head..].starts_with("0x");

                head + line[head..]
                    .bytes()
                    .position(|b| {
                        state = match (&state, b) {
                            (Integral, b'.') => Fractional,
                            (Integral | Fractional, b'e') | (Hexadecimal, b'p') => ExponentSign,
                            (ExponentSign, b'-' | b'0'..=b'9') => Exponent,
                            (Integral, b'x') if ox => Hexadecimal,

                            (Integral, b'0'..=b'9') => Integral,
                            (Fractional, b'0'..=b'9') => Fractional,
                            (Exponent, b'0'..=b'9') => Exponent,
                            (Hexadecimal, b'0'..=b'9' | b'a'..=b'f' | b'A'..=b'F') => Hexadecimal,

                            _ => return true,
                        };
                        false
                    })
                    .unwrap_or(line.len() - head)
            }

            [b'=', b'~' | b'<' | b'>', b'=', rest @ ..]
            | [b'.', b'.', b'.', rest @ ..]
            | [b'.', b'.', rest @ ..]
            | [b'+' | b'-' | b'*' | b'/' | b'%' | b'^' | b'#' | b'<' | b'>' | b'=' | b'(' | b')'
            | b'{' | b'}' | b'[' | b']' | b';' | b':' | b',' | b'.', rest @ ..] => {
                let punct = &line[head..line.len() - rest.len()];
                head + punct.len()
            }

            _ => {
                let nonalnum = line[head..]
                    .char_indices()
                    .find(|p| '_' != p.1 && !p.1.is_alphanumeric())
                    .map(|p| p.0)
                    .unwrap_or(line.len() - head);

                if 0 != nonalnum {
                    head + nonalnum
                } else {
                    head + line[head..]
                        .char_indices()
                        .find(|p| {
                            b"#()*+,-./0123456789:;<=>[]^_{}~".contains(&(p.1 as u8))
                                || p.1.is_alphanumeric()
                        })
                        .map(|p| p.0)
                        .unwrap_or(line.len() - head)
                }
            }
        };

        parts.push(line[head..ahead].to_string());

        if in_part.is_none() {
            if let Some(point) = point {
                // token     t o k e n      token
                //      head[         [ahead
                if point < head {
                    in_part = Some(parts.len() - 1);
                }
                if (head..ahead).contains(&point) {
                    in_part = Some(parts.len() - 1);
                }
            }
        }
        head = ahead;
    }

    //let in_part = in_part.unwrap_or(todo!());
    PromptSplitInfo { parts, in_part }
}

// TODO: this should be moved into navigate so it can integrate better with:
//      * watchers updates (think eg inotify for fs-based)
//      * key mapping? tho we dont have mode mapping and dont plan to
//      * redrawing the breadcrumbs line after completion session
//      * base inputs and outputs on the same instance of the same streams
//      * ... idk
/// A readline-like prompt.
///
/// The cursor is expected to be on the first column already. `ps` is the prompt, it is used
/// without a trailing space. The completion function receive the current line of input and the
/// *character position* of the point. The history is of course not edited, it is caller choice to
/// append the last line to it.
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
                if 0 == at {
                    if s.is_empty() {
                        return None;
                    }
                } else {
                    at -= 1;
                    s.remove(at);
                    write!(output, "\x08\x1b[P").ok()?;
                }
            }
            [0x09] => {
                match &complete(&s.iter().collect::<String>(), at)[..] {
                    [] => (),
                    [single] => todo!("insert completion: {single:?}"),
                    hints => {
                        write!(output, "\r\x1b[A\x1b[K").ok()?;
                        // TODO: limit to term width
                        write!(output, "{}", hints.join(" ")).ok()?;
                        write!(output, "\n\r{ps}").ok()?;
                        if 0 < at {
                            write!(output, "\x1b[{}C", at).ok()?;
                        }
                    }
                }
            }
            [0x0a | 0x0d] => return Some(s.into_iter().collect()),
            [0x0b] => {
                write!(output, "\x1b[{}P", s.len() - at).ok()?;
                s.truncate(at);
            }
            [0x0c] => {
                write!(output, "\x1b[G\x1b[K{ps}").ok()?;
                s.iter().try_for_each(|c| write!(output, "{c}")).ok()?;
                if at < s.len() {
                    write!(output, "\x1b[{}D", s.len() - at).ok()?;
                }
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
                if history.len() - 1 == in_hist {
                    history[in_hist] = s.into_iter().collect();
                }
                in_hist -= 1;
                s = history[in_hist].chars().collect();
                s.iter().try_for_each(|c| write!(output, "{c}")).ok()?;
                at = s.len();
            }
            [0x15] => {
                write!(output, "\x1b[{at}D\x1b[{at}P").ok()?;
                s.drain(..at);
                at = 0;
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
    ($split:ident, $line:literal, $point:literal, $parts:expr, $in_part:expr $(,)?) => {
        let r = $split($line, Some($point));
        assert_eq!(r.parts, $parts, stringify!($split($line, $point)));
        assert_eq!(r.in_part, Some($in_part), stringify!($split($line, $point)));
    };
    ($split:ident, $line:literal, $parts:expr $(,)?) => {
        let parts = $split($line, None).parts;
        assert_eq!(parts, $parts, stringify!($split($line)));
    };
}

#[cfg(test)]
#[test]
fn test_split() {
    assert_parts!(shell_like_split, "", [""]);
    assert_parts!(shell_like_split, " ", [""]);
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
    assert_parts!(shell_like_split, "one two", 0, ["one", "two"], 0);
    assert_parts!(shell_like_split, "one two", 3, ["one", "two"], 0);
    assert_parts!(shell_like_split, "one two", 4, ["one", "two"], 1);
    assert_parts!(shell_like_split, "one two", 6, ["one", "two"], 1);
    assert_parts!(shell_like_split, "one two", 7, ["one", "two"], 1);
    assert_parts!(shell_like_split, " one two", 0, ["", "one", "two"], 0);
    assert_parts!(shell_like_split, "one  two", 4, ["one", "", "two"], 1);
    assert_parts!(shell_like_split, "one two ", 8, ["one", "two", ""], 2);

    assert_parts!(lua_tokens_split, "", [""; 0]);
    assert_parts!(
        lua_tokens_split,
        "function len(t) return t.n or #t end",
        ["function", "len", "(", "t", ")", "return", "t", ".", "n", "or", "#", "t", "end"],
    );
    assert_parts!(
        lua_tokens_split,
        r#"
 -- hello
--[[
hi]]
     --[==> swurd!
[[a]]--[=halo=]
--[===[ uuu ]] ]==] ]===]
        "#,
        [
            "-- hello\n",
            "--[[\nhi]]",
            "--[==> swurd!\n",
            "[[a]]",
            "--[=halo=]\n",
            "--[===[ uuu ]] ]==] ]===]"
        ],
    );
    assert_parts!(
        lua_tokens_split,
        ".5 ..5 ...5 3.0 53e6 1.4e-2-0xabc+1",
        [".5", "..", "5", "...", "5", "3.0", "53e6", "1.4e-2", "-", "0xabc", "+", "1"],
    );
    assert_parts!(lua_tokens_split, "one two", 0, ["one", "two"], 0);
    assert_parts!(lua_tokens_split, "one two", 3, ["one", "two"], 0);
    assert_parts!(lua_tokens_split, "one two", 4, ["one", "two"], 1);
    assert_parts!(lua_tokens_split, "one two", 6, ["one", "two"], 1);
    assert_parts!(lua_tokens_split, "one two", 7, ["one", "two"], 1);
    assert_parts!(lua_tokens_split, " one two", 0, ["", "one", "two"], 0);
    assert_parts!(lua_tokens_split, "one  two", 4, ["one", "", "two"], 1);
    assert_parts!(lua_tokens_split, "one two ", 8, ["one", "two", ""], 2);
}
