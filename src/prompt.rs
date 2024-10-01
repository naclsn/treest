use std::io::Write;
use std::mem;

fn split<T>(line: &[char], point: usize, complete: impl Fn(Vec<&str>, usize) -> T) -> T {
    let mut args = Vec::new();
    let mut curr = String::new();
    let mut in_arg = 0;

    enum State {
        Word,
        Blank,
        SingleQuote,
        DoubleQuote,
    }
    use State::*;

    let word = line.is_empty() || !line[0].is_whitespace();
    let mut state = if word { Word } else { Blank };

    let mut chars = line.iter().copied().enumerate();
    while let Some((k, c)) = chars.next() {
        match state {
            Word | Blank if '\'' == c => state = SingleQuote,
            Word | Blank if '\"' == c => state = DoubleQuote,

            Word if '\\' == c => match chars.next() {
                Some((_, c)) => curr.push(c),
                None => break,
            },
            Word if c.is_whitespace() => {
                if 0 == in_arg && point < k {
                    in_arg = args.len();
                }
                args.push(mem::take(&mut curr));
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
                Some((_, 'e')) => curr.push('\x1b'),
                Some((_, c)) => curr.push(c),
                None => break,
            },
            DoubleQuote => curr.push(c),
        }
    }
    if !matches!(state, Blank) {
        args.push(curr);
    }

    complete(args.iter().map(String::as_str).collect(), in_arg)
}

pub fn prompt(
    ps: &str,
    input: impl IntoIterator<Item = u8>,
    mut output: impl Write,
    complete: impl Fn(Vec<&str>, usize) -> Vec<String>,
) -> Option<String> {
    write!(output, "{ps}").ok()?;

    let mut at = 0;
    let mut s = Vec::new();

    let mut pend = Vec::new();
    let mut input = input.into_iter();
    while let Some(key) = input.next() {
        pend.push(key);
        match &pend[..] {
            b"\x1bb" => todo!(),
            b"\x1bd" => todo!(),
            b"\x1bf" => todo!(),
            b"\x1b\x1b" => return None,
            b"\x1b\x7f" => todo!(),

            [0x01] | b"\x1b[H" => {
                if 0 < at {
                    write!(output, "\x1b[{at}D").ok()?;
                    at = 0;
                }
            }
            [0x02] | b"\x1b[D" => {
                if 0 < at {
                    write!(output, "\x08").ok()?;
                    at -= 1;
                }
            }
            [0x03] => return None,
            [0x04] | b"\x1b[3~" => {
                if at < s.len() {
                    s.remove(at);
                    write!(output, "\x1b[P").ok()?;
                }
            }
            [0x05] | b"\x1b[F" => {
                if at < s.len() {
                    write!(output, "\x1b[{}C", s.len() - at).ok()?;
                    at = s.len();
                }
            }
            [0x06] | b"\x1b[C" => {
                if at < s.len() {
                    write!(output, "{}", s[at]).ok()?;
                    at += 1;
                }
            }
            [.., 0x07] => pend.clear(),
            [0x08 | 127] => {
                if 0 < at {
                    at -= 1;
                    s.remove(at);
                    write!(output, "\x08\x1b[P").ok()?;
                }
            }
            [0x09] => {
                let hints = split(&s, at, complete);
                todo!("completion hints: {hints:?}");
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
            [0x15] => {
                write!(output, "\x1b[{at}D\x1b[{at}P").ok()?;
                s.drain(..at);
            }

            b"\x1b" | b"\x1b[" | [0x1b, b'[', b'0'..=b'9'] => continue,
            [0x1b, ..] => (),

            [b' '..=255] => {
                let u = key as u32;
                let c = char::from_u32(match key {
                    0b11000000..=0b11011111 => {
                        let x = input.next()? as u32;
                        (u & 31) << 6 | (x & 63)
                    }
                    0b11100000..=0b11101111 => {
                        let (x, y) = (input.next()? as u32, input.next()? as u32);
                        (u & 15) << 12 | (x & 63) << 6 | (y & 63)
                    }
                    0b11110000..=0b11110111 => {
                        let (x, y, z) = (
                            input.next()? as u32,
                            input.next()? as u32,
                            input.next()? as u32,
                        );
                        (u & 7) << 18 | (x & 63) << 12 | (y & 63) << 6 | (z & 63)
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
macro_rules! assert_args {
    ($line:literal, $args:expr) => {
        let chars: Box<_> = $line.chars().collect();
        split(&chars, 0, |args, _| assert_eq!(args, $args, $line));
    };
}

#[cfg(test)]
#[test]
fn test_split() {
    assert_args!("", [""]);
    assert_args!("coucou ", ["coucou"]);
    assert_args!(" this\tis\ntest    text", ["this", "is", "test", "text"]);
    assert_args!(
        r#" 'quo'ted and dis"joi'\n"'\t"' ye"\""y   "#,
        ["quoted", "and", "disjoi'\n\\t\"", "ye\"y"]
    );
    assert_args!("it's fine", ["its fine"]);
}
