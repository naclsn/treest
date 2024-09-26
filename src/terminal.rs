pub use self::plat::*;

#[cfg(unix)]
pub mod plat {
    use libc::{self, termios as Termios, winsize as Winsize};
    use std::io::Error as IoError;

    pub struct Restore(Termios);

    pub fn raw() -> Result<Restore, IoError> {
        unsafe {
            let mut attr: Termios = std::mem::MaybeUninit::zeroed().assume_init();
            if libc::tcgetattr(libc::STDOUT_FILENO, &mut attr) < 0 {
                Err(IoError::last_os_error())
            } else {
                let r = Restore(attr);
                libc::cfmakeraw(&mut attr);
                libc::tcsetattr(libc::STDOUT_FILENO, libc::TCSANOW, &attr);
                Ok(r)
            }
        }
    }

    impl Restore {
        pub fn restore(self) {
            unsafe {
                libc::tcsetattr(libc::STDOUT_FILENO, libc::TCSANOW, &self.0);
            }
        }
    }

    pub fn size() -> Result<(u16, u16), IoError> {
        unsafe {
            let mut winsz: Winsize = std::mem::MaybeUninit::zeroed().assume_init();
            if libc::ioctl(
                libc::STDOUT_FILENO,
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
