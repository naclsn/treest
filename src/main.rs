mod lua;
mod navigate;
mod options;
mod prompt;
mod provider;
mod terminal;
mod tree;

#[macro_export]
macro_rules! include_etc {
    ($file:expr $(,)?) => {
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/etc/", $file))
    };
}

mod log {
    pub(crate) fn log(_file: &str, _line: u32, _column: u32, _fmt: std::fmt::Arguments) {
        #[cfg(feature = "log")]
        {
            use std::{fs::File, io::Write, sync::OnceLock, thread, time::SystemTime};
            use time::OffsetDateTime;
            static LOG: OnceLock<File> = OnceLock::new();
            let mut log = LOG.get_or_init(|| std::fs::File::create("/tmp/treest.log").unwrap());
            let time: OffsetDateTime = SystemTime::now().into();
            let this = thread::current();
            let name = this.name().unwrap_or("<unknown>");
            _ = write!(log, "\x1b[36m{time}\x1b[m ");
            _ = write!(log, "\x1b[35m{_file}:{_line}:{_column}\x1b[m ");
            _ = writeln!(log, "in thread {name}");
            _ = log.write_fmt(_fmt);
            _ = writeln!(log);
        }
    }
}
#[macro_export]
macro_rules! log {
    ($fmt:expr) => {
        $crate::log::log(file!(), line!(), column!(), format_args!($fmt))
    };
    ($fmt:expr, $($args:tt)*) => {
        $crate::log::log(file!(), line!(), column!(), format_args!($fmt, $($args)*))
    };
}

fn main() -> anyhow::Result<()> {
    Ok(options::Options::parse_env()?.instanciate()?.main_loop()?)
}
