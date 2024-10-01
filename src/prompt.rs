use std::io::Write;

fn split(line: &[char], complete: impl Fn(Vec<&str>) -> Vec<String>) -> Vec<String> {
    let args: Vec<_> = line
        .split(|c| *c == ' ')
        .map(|s| s.iter().collect())
        .collect();
    complete(args.iter().map(String::as_str).collect())
}

pub fn prompt(
    ps: &str,
    input: impl IntoIterator<Item = u8>,
    mut output: impl Write,
    complete: impl Fn(Vec<&str>) -> Vec<String>,
) -> Option<String> {
    write!(output, "{ps}").ok()?;

    let mut at = 0;
    let mut s = Vec::new();

    let mut pend = Vec::new();
    let mut input = input.into_iter();
    while let Some(key) = input.next() {
        pend.push(key);
        match &pend[..] {
            //b"\x1bb" => ..
            //b"\x1bd" => ..
            //b"\x1bf" => ..
            b"\x1b\x1b" => return None,
            //b"\x1b\x7f" => ..

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
            [0x08] => {
                if 0 < at {
                    at -= 1;
                    s.remove(at);
                    write!(output, "\x08\x1b[P").ok()?;
                }
            }
            [0x09] => {
                let hints = split(&s, complete);
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

            [127] => {
                if 0 < at {
                    at -= 1;
                    s.remove(at);
                    write!(output, "\x08\x1b[P").ok()?;
                }
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
