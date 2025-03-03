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
    return Some(());
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

#[cfg(test)]
macro_rules! assert_trans {
    ($text:literal, None) => {
        let trans = keytrans($text);
        assert_eq!(trans, None, $text);
    };

    ($text:literal, $trans:expr) => {
        let trans = keytrans($text).expect($text);
        assert_eq!(trans, $trans, $text);
    };
}

#[cfg(test)]
#[test]
fn test_keytrans() {
    assert_trans!("xlty", b"xlty");
    assert_trans!("x<lty", None);
    assert_trans!("x<lt>y", b"x<y");
    assert_trans!("x<LT>y", b"x<y");
    assert_trans!("<C-x><C-e>", [0x18, 0x05]);
    assert_trans!("<C-x><M-C-e>", [0x18, 0x1b, 0x05]);
    assert_trans!("<C-x><M-A-C-e>", None);
}
