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
    pub(crate) fn log(_fmt: std::fmt::Arguments) {
        #[cfg(feature = "log")]
        {
            use std::{fs::File, io::Write, sync::OnceLock};
            static LOG: OnceLock<File> = OnceLock::new();
            _ = LOG
                .get_or_init(|| std::fs::File::create("/tmp/treest.log").unwrap())
                .write_fmt(_fmt);
        }
    }
}
#[macro_export]
macro_rules! log {
    ($fmt:expr) => {
        $crate::log::log(format_args!($fmt))
    };
    ($fmt:expr, $($args:tt)*) => {
        $crate::log::log(format_args!($fmt, $($args)*))
    };
}

fn main() -> anyhow::Result<()> {
    Ok(options::Options::parse_env()?.instanciate()?.main_loop()?)
}
