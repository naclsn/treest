//pub struct Feature {
//    name: &'static str,
//    on_set:
//    on_rst:
//}

pub struct Options {
    pub(super) mouse: bool,
    pub(super) altscreen: bool,
    pub(super) pretty: bool,
    pub(super) onlychild: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            mouse: true,
            altscreen: true,
            pretty: true,
            onlychild: true,
        }
    }
}

macro_rules! option {
    ($self:ident.$name:ident: bool; $no:ident, $query:ident, $toggle:ident) => {{
        if $query {
            return Some(format!(
                "{}{}",
                if $self.$name { "" } else { "no" },
                stringify!($name)
            ));
        }
        $self.$name = match ($self.$name, $no, $toggle) {
            (false, false, false) | (false, _, true) => true,
            (true, true, false) | (true, _, true) => false,
            _ => return None,
        };
        $self.$name
    }};

    ($self:ident.$name:ident: str; $no:ident, $query:ident, $equal:ident) => {{}};
}

impl Options {
    /// returns something if there is something to message
    pub fn update(&mut self, mut opt: &str) -> Option<String> {
        let no = opt.strip_prefix("no").map(|o| opt = o).is_some();
        let query = opt.strip_suffix("?").map(|o| opt = o).is_some();
        let toggle = opt.strip_suffix("!").map(|o| opt = o).is_some();
        let (opt, _equal) = opt.split_once("=").unwrap_or((opt, ""));

        match opt {
            "mouse" => {
                if option!(self.mouse: bool; no, query, toggle) {
                    eprint!("\x1b[?1000h");
                } else {
                    eprint!("\x1b[?1000l");
                }
            }
            "alts" | "altscreen" => {
                if option!(self.altscreen: bool; no, query, toggle) {
                    eprint!("\x1b[?1049h");
                } else {
                    eprint!("\x1b[?1049l");
                }
            }
            "pretty" => {
                option!(self.pretty: bool; no, query, toggle);
                return Some("NIY: pretty".to_string());
            }
            "onchl" | "onlychild" => {
                option!(self.onlychild: bool; no, query, toggle);
                return Some("NIY: onlychild".to_string());
            }
            _ => return Some(format!("unknown option: {opt}")),
        }

        None
    }
}
