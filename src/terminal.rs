pub use self::plat::*;

#[cfg(unix)]
pub mod plat {
    use libc::{self, termios as Termios, winsize as Winsize};
    use std::io::Error as IoError;

    pub struct Restore(Termios);

    pub fn raw() -> Result<Restore, IoError> {
        unsafe {
            let mut attr: Termios = std::mem::MaybeUninit::zeroed().assume_init();
            if libc::tcgetattr(libc::STDERR_FILENO, &mut attr) < 0 {
                Err(IoError::last_os_error())
            } else {
                let r = Restore(attr);
                libc::cfmakeraw(&mut attr);
                libc::tcsetattr(libc::STDERR_FILENO, libc::TCSANOW, &attr);
                Ok(r)
            }
        }
    }

    impl Restore {
        pub fn restore(self) {
            unsafe {
                libc::tcsetattr(libc::STDERR_FILENO, libc::TCSANOW, &self.0);
            }
        }
    }

    pub fn size() -> Result<(u16, u16), IoError> {
        unsafe {
            let mut winsz: Winsize = std::mem::MaybeUninit::zeroed().assume_init();
            if libc::ioctl(
                libc::STDERR_FILENO,
                libc::TIOCGWINSZ,
                &mut winsz as *mut Winsize,
            ) < 0
            {
                Err(IoError::last_os_error())
            } else {
                Ok((winsz.ws_row, winsz.ws_col))
            }
        }
    }
}

#[cfg(windows)]
pub mod plat {
    use std::io::Error as IoError;
    use std::ptr;
    use winapi::{
        fileapi::{self, CreateFileW},
        um::{
            consoleapi, handleapi,
            wincon::{self, CONSOLE_SCREEN_BUFFER_INFO as ConsoleScreenInfo},
            winnt::{self, HANDLE as Handle},
        },
    };

    unsafe fn handle() -> Result<Handle, IoError> {
        let con: Vec<u16> = "CONOUT$\0".encode_utf16().collect();
        let handle = CreateFileW(
            con.as_ptr(),
            winnt::GENERIC_READ | winnt::GENERIC_WRITE,
            winnt::FILE_SHARE_READ | winnt::FILE_SHARE_WRITE,
            ptr::null_mut(),
            fileapi::OPEN_EXISTING,
            0,
            ptr::null_mut(),
        );
        if handleapi::INVALID_HANDLE_VALUE == handle {
            Err(IoError::last_os_error())
        } else {
            Ok(handle)
        }
    }

    pub struct Restore(Handle, u32);

    pub fn raw() -> Result<Restore, IoError> {
        unsafe {
            let handle = handle()?;
            let mode = 0;
            if 0 == consoleapi::GetConsoleMode(handle, &mut mode) {
                Err(IoError::last_os_error())
            } else {
                consoleapi::SetConsoleMode(handle, mode & !wincon::NOT_RAW_MODE_MASK);
                Ok(Restore(handle, mode))
            }
        }
    }

    impl Restore {
        pub fn restore(self) {
            consoleapi::SetConsoleMode(self.0, self.1);
        }
    }

    pub fn size() -> Result<(u16, u16), IoError> {
        unsafe {
            let handle = handle()?;
            let mut info: ConsoleScreenInfo = std::mem::MaybeUninit::zeroed().assume_init();
            if 0 == wincon::GetConsoleScreenBufferInfo(handle, &mut info) {
                Err(IoError::last_os_error())
            } else {
                Ok((
                    (info.srWindow.Bottom - info.srWindow.Top + 1) as u16,
                    (info.srWindow.Right - info.srWindow.Left + 1) as u16,
                ))
            }
        }
    }
}

pub fn cursor_on() {
    eprint!("\x1b[?25h");
}
pub fn mouse_on() {
    eprint!("\x1b[?1000h");
}
pub fn altscreen_on() {
    eprint!("\x1b[?1049h");
}

pub fn cursor_off() {
    eprint!("\x1b[?25l");
}
pub fn mouse_off() {
    eprint!("\x1b[?1000l");
}
pub fn altscreen_off() {
    eprint!("\x1b[?1049l");
}

fn keytrans1(slice: &[u8], r: &mut Vec<u8>) -> Option<()> {
    match slice {
        b"NUL" => r.push(0),
        b"BS" => r.push(0x7f),
        b"TAB" => r.push(b'\t'),
        b"NL" => r.push(b'\n'),
        b"CR" | b"RETURN" | b"ENTER" => r.push(b'\r'),
        b"ESC" | b"ESCAPE" => r.push(0x1b),
        b"SPACE" => r.push(b' '),
        b"LT" => r.push(b'<'),
        b"GT" => r.push(b'>'),
        b"BSLASH" => r.push(b'\\'),
        b"BAR" => r.push(b'|'),
        b"CSI" => r.extend(b"\x1b["),

        b"UP" => r.extend(b"\x1b[A"),
        b"DOWN" => r.extend(b"\x1b[B"),
        b"RIGHT" => r.extend(b"\x1b[C"),
        b"LEFT" => r.extend(b"\x1b[D"),

        b"HOME" => r.extend(b"\x1b[H"),
        b"END" => r.extend(b"\x1b[F"),
        b"INS" | b"INSERT" => r.extend(b"\x1b[2~"),
        b"DEL" | b"DELETE" => r.extend(b"\x1b[3~"),
        b"PAGEUP" => r.extend(b"\x1b[5~"),
        b"PAGEDOWN" => r.extend(b"\x1b[6~"),

        [b'C', b'-', k @ b'@'..=b'_'] => r.push(*k ^ 0b1000000),
        [b'M' | b'A', b'-', b'M' | b'A', b'-', ..] => return None,
        [b'M' | b'A', b'-', rest @ ..] => {
            r.push(0x1b);
            keytrans1(rest, r)?;
        }

        b"LEFTMOUSE" => r.extend([0x1b, b'[', b'M', 32, b' ', b' ']),
        b"RIGHTMOUSE" => r.extend([0x1b, b'[', b'M', 34, b' ', b' ']),
        b"SCROLLWHEELUP" | b"FORWARDWHEEL" => r.extend([0x1b, b'[', b'M', 96, b' ', b' ']),
        b"SCROLLWHEELDOWN" | b"BACKWARDWHEEL" => r.extend([0x1b, b'[', b'M', 97, b' ', b' ']),
        b"UPMOUSE" => r.extend([0x1b, b'[', b'M', 35, b' ', b' ']),

        _ => return None,
    }
    Some(())
}

pub fn keytrans(text: &str) -> Option<Vec<u8>> {
    let mut r = vec![];

    let mut iter = text.char_indices();
    let mut at = 0;
    loop {
        match iter.find(|(_, chr)| '<' == *chr) {
            Some((start, _)) => {
                let (end, _) = iter.find(|(_, chr)| '>' == *chr)?;
                r.extend(text[at..start].as_bytes());
                at = end + 1;
                keytrans1(
                    &text[start + 1..end].as_bytes().to_ascii_uppercase()[..],
                    &mut r,
                )?;
            }
            None => {
                r.extend(text[at..text.len()].as_bytes());
                break;
            }
        }
    }

    Some(r)
}

macro_rules! extend_match_start {
    {
        $subj:ident $re:ident $r:ident
        $( [$($with:expr),+] => $then:expr ,)+
    } => {
        match $subj {
            $( [$($with,)+ ..] => {
                if !$re { $r.push('<'); }
                $r.push_str($then);
                if !$re { $r.push('>'); }
                return [$($with),+].len();
            } )+
            _ => (),
        }
    }
}

fn keyseqstr1(slice: &[u8], re: bool, r: &mut String) -> usize {
    if slice.is_empty() {
        return 0;
    }

    extend_match_start! { slice re r
        [0x1b, b'[', b'M', 35, b' ', b' '] => "UpMouse",
        [0x1b, b'[', b'M', 97, b' ', b' '] => "BackwardWheel",
        [0x1b, b'[', b'M', 96, b' ', b' '] => "ForwardWheel",
        [0x1b, b'[', b'M', 34, b' ', b' '] => "RightMouse",
        [0x1b, b'[', b'M', 32, b' ', b' '] => "LeftMouse",

        [0x1b, b'[', b'6', b'~'] => "PageDown",
        [0x1b, b'[', b'5', b'~'] => "PageUp",
        [0x1b, b'[', b'3', b'~'] => "Delete",
        [0x1b, b'[', b'2', b'~'] => "Insert",
        [0x1b, b'[', b'F'] => "End",
        [0x1b, b'[', b'H'] => "Home",

        [0x1b, b'[', b'D'] => "Left",
        [0x1b, b'[', b'C'] => "Right",
        [0x1b, b'[', b'B'] => "Down",
        [0x1b, b'[', b'A'] => "Up",

        [0x1b, b'['] => "CSI",
        [b'|'] => "Bar",
        [b'\\'] => "Bslash",
        [b'>'] => "gt",
        [b'<'] => "lt",
        [b' '] => "Space",
        [b'\r'] => "CR",
        [b'\n'] => "NL",
        [b'\t'] => "Tab",
        [0x7f] => "BS",
        [0] => "Nul",
    }

    if 0x1b == slice[0] {
        if !re {
            r.push('<');
        }
        let mut n = 1;
        if 1 == slice.len() {
            r.push_str("Esc");
        } else {
            r.push_str("M-");
            n += keyseqstr1(&slice[1..], true, r)
        }
        if !re {
            r.push('>');
        }
        return n;
    }

    if slice[0].is_ascii_control() {
        if !re {
            r.push('<');
        }
        r.push_str("C-");
        r.push((slice[0] ^ 0b1000000) as char);
        if !re {
            r.push('>');
        }
        return 1;
    }

    if slice[0].is_ascii_graphic() {
        r.push(slice[0] as char);
        return 1;
    }

    // meh.. this part should be reached quite rarely (if at all? >0x7f through keytrans?)
    if let Ok(as_str) = std::str::from_utf8(slice) {
        if let Some((len, chr)) = as_str.char_indices().nth(1) {
            r.push(chr);
            return len;
        }
        return 1;
    }

    if !re {
        r.push('<');
    }
    let h = (slice[0] >> 4) & 0xf;
    let l = slice[0] & 0xf;
    r.push((if h < 10 { b'0' } else { b'a' } + h) as char);
    r.push((if l < 10 { b'0' } else { b'a' } + l) as char);
    if !re {
        r.push('>');
    }
    1
}

pub fn keyseqstr(seq: &[u8]) -> String {
    let mut r = String::new();

    let mut k = 0;
    while k < seq.len() {
        k += keyseqstr1(&seq[k..], false, &mut r);
    }

    r
}

pub fn keyseqstr_each(seq: &[u8]) -> String {
    seq.iter().map(|b| keyseqstr(&[*b])).collect()
}

#[cfg(test)]
macro_rules! assert_trans {
    ($text:literal => None) => {
        let trans = keytrans($text);
        assert_eq!(trans, None, $text);
    };

    ($text:literal => $trans:expr) => {
        let trans = keytrans($text).expect($text);
        assert_eq!(trans, $trans, stringify!($text));
    };

    ($text:literal <= $trans:expr) => {
        let text = keyseqstr($trans);
        assert_eq!(text, $text, stringify!($trans));
    };
}

#[cfg(test)]
#[test]
fn test_keytrans() {
    assert_trans!("xlty" => b"xlty");
    assert_trans!("x<lty" => None);
    assert_trans!("x<lt>y" => b"x<y");
    assert_trans!("x<LT>y" => b"x<y");
    assert_trans!("<C-x><C-e>" => &[0x18, 0x05]);
    assert_trans!("<C-X><M-C-e>" => &[0x18, 0x1b, 0x05]);
    assert_trans!("<C-x><M-A-C-e>" => None);

    assert_trans!("xlty" <= b"xlty");
    assert_trans!("x<lt>y" <= b"x<y");
    assert_trans!("<C-X><C-E>" <= &[0x18, 0x05]);
    assert_trans!("<C-X><M-C-E>" <= &[0x18, 0x1b, 0x05]);
}
