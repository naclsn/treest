use std::io::Write;

fn split(line: &str) -> Vec<&str> {
    line.split(' ').collect()
}

pub fn prompt(
    ps: &str,
    input: impl IntoIterator<Item = u8>,
    mut output: impl Write,
    complete: impl Fn(Vec<&str>) -> Vec<String>,
) -> Option<String> {
    write!(output, "{ps}").ok()?;

    let mut at = 0usize;
    let mut s = Vec::new();

    let mut pend = Vec::new();
    for key in input {
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
                    write!(output, "{}", s[at] as char).ok()?;
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
                let hints = complete(split(&String::from_utf8_lossy(&s)));
                todo!("completion hints: {hints:?}");
            }
            [0x0a | 0x0d] => return Some(unsafe { String::from_utf8_unchecked(s) }),
            [0x0b] => {
                write!(output, "\x1b[{}P", s.len() - at).ok()?;
                s.truncate(at);
            }
            [0x0c] => {
                let l = String::from_utf8_lossy(&s);
                write!(output, "\x1b[G\x1b[K{ps}{l}\x1b[{}D", s.len() - at).ok()?;
            }
            [0x15] => {
                write!(output, "\x1b[{at}D\x1b[{at}P").ok()?;
                s.drain(..at);
            }

            [b' '..=b'~'] => {
                write!(output, "\x1b[@{}", key as char).ok()?;
                s.insert(at, key);
                at += 1;
            }
            [127] => {
                if 0 < at {
                    at -= 1;
                    s.remove(at);
                    write!(output, "\x08\x1b[P").ok()?;
                }
            }

            b"\x1b" | b"\x1b[" | [0x1b, b'[', b'0'..=b'9'] => continue,
            [0x1b, ..] => pend.clear(),

            _ => (),
        }
        pend.clear();
    }

    None
}
