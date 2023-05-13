use glob::Pattern;
use lscolors::LsColors;
use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    collections::HashSet,
    fmt,
    fs::{metadata, read_link, read_to_string, symlink_metadata, Metadata},
    io,
    path::{Path, PathBuf},
};
use tui::style::{Color, Modifier, Style};

#[derive(Debug, Clone, Copy)]
pub enum Movement {
    Forward = 1,
    Backward = -1,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub enum SortingProp {
    None,
    Name,
    Size,
    Extension,
    ATime,
    MTime,
    CTime,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct Sorting {
    pub prop: SortingProp,
    pub dirs_first: bool,
}

impl Sorting {
    pub fn new(prop: SortingProp, dirs_first: bool) -> Sorting {
        Sorting { prop, dirs_first }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Filtering {
    Pattern(String),                 // XXX: cannot serialize Pattern
    IgnoreFile(String, Vec<String>), // same as above (the Vec)
}

impl PartialEq for Filtering {
    fn eq(&self, other: &Filtering) -> bool {
        match (self, other) {
            (Filtering::Pattern(a), Filtering::Pattern(b)) => a == b,
            (Filtering::IgnoreFile(a, _), Filtering::IgnoreFile(b, _)) => a == b,
            _ => false,
        }
    }
}

impl fmt::Display for Filtering {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Filtering::Pattern(pat) => write!(f, "{pat}"),
            Filtering::IgnoreFile(name, pats) => {
                write!(f, "# {name} ({} patterns)\n{}", pats.len(), pats.join("\n"))
            }
        }
    }
}
impl Filtering {
    pub fn new_pattern(pat: String) -> Filtering {
        Filtering::Pattern(pat)
    }

    pub fn new_ignore_file(name: &str) -> Option<Filtering> {
        let Ok(text) = read_to_string(name) else { return None; };
        Some(Filtering::IgnoreFile(
            name.to_string(),
            text.lines()
                .map(|it| it.trim())
                .filter(|it| !it.is_empty() && !it.starts_with('#') && !it.starts_with('!')) // YYY: does not handle negative patterns
                .map(|it| it.replace('\\', ""))
                .collect(),
        ))
    }

    fn _matches_one(pat: &str, node: &Node) -> bool {
        let pat = if let Some(bla) = pat.strip_suffix('/') {
            if !node.is_dir() {
                return false;
            }
            bla
        } else {
            pat
        };
        if let Ok(p) = Pattern::new(pat) {
            return p.matches(node.file_name());
        }
        false
    }

    pub fn matches(&self, node: &Node) -> bool {
        match self {
            Filtering::Pattern(pat) => Filtering::_matches_one(pat, node),
            Filtering::IgnoreFile(_, pats) => {
                pats.iter().any(|pat| Filtering::_matches_one(pat, node))
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub enum FileKind {
    NamedPipe,
    CharDevice,
    BlockDevice,
    Regular,
    Socket,
    Executable,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum NodeInfo {
    Dir { loaded: bool, children: Vec<Node> },
    Link { target: Result<Box<Node>, PathBuf> },
    File { kind: FileKind },
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Node {
    path: PathBuf,
    #[serde(skip_serializing, skip_deserializing)]
    meta: Option<Metadata>,
    info: NodeInfo,
}

impl fmt::Display for Node {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let depth = f.precision().unwrap_or(0);
        let indent = "   ".repeat(depth);

        let name = self.path.file_name().unwrap().to_str().unwrap();

        match &self.info {
            NodeInfo::Dir { loaded, children } => {
                write!(f, "{indent}{name}/",)?;
                if *loaded {
                    if children.is_empty() {
                        writeln!(f, " (/)")
                    } else {
                        writeln!(f)?;
                        children
                            .iter()
                            .try_for_each(|ch| write!(f, "{ch:.*}", depth + 1))
                    }
                } else {
                    writeln!(f)
                }
            }

            NodeInfo::Link { target } => {
                write!(f, "{indent}{name}@ -> ")?;
                match target {
                    Ok(node) => write!(f, "{node}"),
                    Err(path) => write!(f, "~{}~", path.to_string_lossy()),
                }
            }

            NodeInfo::File { kind } => match kind {
                FileKind::NamedPipe => writeln!(f, "{indent}{name}|"),
                FileKind::Socket => writeln!(f, "{indent}{name}="),
                FileKind::Executable => writeln!(f, "{indent}{name}*"),
                _ => writeln!(f, "{indent}{name}"),
            },
        }
    }
}

#[cfg(unix)]
impl From<&Metadata> for FileKind {
    fn from(meta: &Metadata) -> FileKind {
        use std::os::unix::fs::FileTypeExt;
        use std::os::unix::fs::PermissionsExt;
        let ft = meta.file_type();
        if ft.is_fifo() {
            FileKind::NamedPipe
        } else if ft.is_char_device() {
            FileKind::CharDevice
        } else if ft.is_block_device() {
            FileKind::BlockDevice
        } else if ft.is_socket() {
            FileKind::Socket
        } else if meta.permissions().mode() & 0o111 != 0 {
            FileKind::Executable
        } else {
            FileKind::Regular
        }
    }
}

#[cfg(windows)]
impl From<&Metadata> for FileKind {
    fn from(meta: &Metadata) -> FileKind {
        FileKind::Regular
    }
}

fn perm_to_string(o: u32) -> String {
    [
        if o >> 2 & 0b1 == 1 { 'r' } else { '-' },
        if o >> 1 & 0b1 == 1 { 'w' } else { '-' },
        if o & 0b1 == 1 { 'x' } else { '-' },
    ]
    .into_iter()
    .collect()
}

#[cfg(unix)]
fn meta_to_string(meta: &Metadata) -> String {
    use std::os::unix::fs::FileTypeExt;
    use std::os::unix::fs::PermissionsExt;

    let mode = meta.permissions().mode();
    let ft = meta.file_type();

    [
        // file type
        if ft.is_block_device() {
            'b'
        } else if ft.is_char_device() {
            'c'
        } else if ft.is_dir() {
            'd'
        } else if ft.is_symlink() {
            'l'
        } else if ft.is_fifo() {
            'p'
        } else if ft.is_socket() {
            's'
        } else {
            '-'
        }
        .to_string(),
        // owner
        perm_to_string(mode >> 6 & 0b111),
        // group
        perm_to_string(mode >> 3 & 0b111),
        // world
        perm_to_string(mode & 0b111),
    ]
    .concat()
}

#[cfg(windows)]
fn meta_to_string(meta: &Metadata) -> String {
    //use std::os::windows::fs::FileTypeExt;
    //use std::os::windows::fs::PermissionsExt;

    let ro = meta.permissions().readonly();
    let ft = meta.file_type();

    [
        // file type
        if ft.is_dir() {
            'd'
        } else if ft.is_symlink() {
            'l'
        } else {
            '-'
        }
        .to_string(),
        // owner
        perm_to_string(0b101 | if ro { 0b000 } else { 0b010 }),
        // group
        perm_to_string(0b101 | if ro { 0b000 } else { 0b010 }),
        // world
        perm_to_string(0b101 | if ro { 0b000 } else { 0b010 }),
    ]
    .concat()
}

fn cmp_in<T, C: Ord>(l: &Option<T>, r: &Option<T>, sel: fn(&T) -> C) -> Ordering {
    l.as_ref()
        .zip(r.as_ref())
        .map_or(Ordering::Equal, |(m, o)| sel(m).cmp(&sel(o)))
}

fn lscolors_to_tui_color(lco: &lscolors::style::Color) -> Color {
    match lco {
        lscolors::style::Color::Black => Color::Black,
        lscolors::style::Color::Red => Color::Red,
        lscolors::style::Color::Green => Color::Green,
        lscolors::style::Color::Yellow => Color::Yellow,
        lscolors::style::Color::Blue => Color::Blue,
        lscolors::style::Color::Magenta => Color::Magenta,
        lscolors::style::Color::Cyan => Color::Cyan,
        lscolors::style::Color::White => Color::Gray,
        lscolors::style::Color::BrightBlack => Color::DarkGray,
        lscolors::style::Color::BrightRed => Color::LightRed,
        lscolors::style::Color::BrightGreen => Color::LightGreen,
        lscolors::style::Color::BrightYellow => Color::LightYellow,
        lscolors::style::Color::BrightBlue => Color::LightBlue,
        lscolors::style::Color::BrightMagenta => Color::LightMagenta,
        lscolors::style::Color::BrightCyan => Color::LightCyan,
        lscolors::style::Color::BrightWhite => Color::White,
        lscolors::style::Color::Fixed(k) => Color::Indexed(*k),
        lscolors::style::Color::RGB(r, g, b) => Color::Rgb(*r, *g, *b),
    }
}

fn lscolors_to_tui_style(lsy: &lscolors::style::Style) -> Style {
    let mut r = Style::default();
    if let Some(fg) = lsy.foreground.as_ref().map(lscolors_to_tui_color) {
        r = r.fg(fg);
    }
    if let Some(bg) = lsy.background.as_ref().map(lscolors_to_tui_color) {
        r = r.bg(bg);
    }
    if lsy.font_style.bold {
        r = r.add_modifier(Modifier::BOLD);
    }
    if lsy.font_style.dimmed {
        r = r.add_modifier(Modifier::DIM);
    }
    if lsy.font_style.italic {
        r = r.add_modifier(Modifier::ITALIC);
    }
    if lsy.font_style.underline {
        r = r.add_modifier(Modifier::UNDERLINED);
    }
    if lsy.font_style.slow_blink {
        r = r.add_modifier(Modifier::SLOW_BLINK);
    }
    if lsy.font_style.rapid_blink {
        r = r.add_modifier(Modifier::RAPID_BLINK);
    }
    if lsy.font_style.reverse {
        r = r.add_modifier(Modifier::REVERSED);
    }
    if lsy.font_style.hidden {
        r = r.add_modifier(Modifier::HIDDEN);
    }
    if lsy.font_style.strikethrough {
        r = r.add_modifier(Modifier::CROSSED_OUT);
    }
    r
}

impl Node {
    pub fn new(path: PathBuf, meta: Metadata) -> io::Result<Node> {
        let info = if meta.is_dir() {
            NodeInfo::Dir {
                loaded: false,
                children: Vec::new(),
            }
        } else if meta.is_symlink() {
            NodeInfo::Link {
                target: {
                    let realpath = read_link(path.clone())?;
                    symlink_metadata(realpath.clone())
                        .and_then(|lnmeta| Node::new(realpath.clone(), lnmeta))
                        .map(Box::new)
                        .map_err(|_| realpath)
                },
            }
        } else {
            NodeInfo::File {
                kind: (&meta).into(),
            }
        };

        Ok(Node {
            path,
            meta: Some(meta),
            info,
        })
    }

    pub fn renew(&self) -> io::Result<Node> {
        let mut node = Node::new(self.path.clone(), metadata(&self.path)?)?;

        // correct for any change, load children if any
        // (does not account for eg. dir became a link)
        #[allow(clippy::single_match)]
        match (&self.info, &mut node.info) {
            // it was a loaded dir; if it still is a dir, load it
            (
                NodeInfo::Dir {
                    loaded: true,
                    children: previous,
                },
                NodeInfo::Dir { loaded, children },
            ) => {
                let mut done = HashSet::<&str>::new();

                children.reserve(previous.len());
                for ch in previous {
                    if let Ok(niw) = ch.renew() {
                        children.push(niw);
                        done.insert(ch.file_name());
                    }
                }

                children.extend(
                    self.path
                        .read_dir()?
                        .filter_map(Result::ok)
                        .filter(|ent| !done.contains(ent.file_name().to_str().unwrap()))
                        .map(|ent| ent.metadata().and_then(|meta| Node::new(ent.path(), meta)))
                        .filter_map(Result::ok),
                );

                *loaded = true;
            }

            // TODO:
            // // it was a link; if it still is,
            // (NodeInfo::Link { target: previous }, NodeInfo::Link { target }) => {
            //     todo!("not sure but maybe something (think link to dir)");
            // }

            // it was a file or an unloaded dir;
            // whatever it is now, noop
            _ => (),
        }

        Ok(node)
    }

    pub fn new_root(path: PathBuf) -> io::Result<Node> {
        let meta = Some(metadata(&path)?);
        Ok(Node {
            path,
            meta,
            info: NodeInfo::Dir {
                loaded: false,
                children: Vec::new(),
            },
        })
    }

    pub fn as_path(&self) -> &Path {
        self.path.as_path()
    }

    pub fn cmp_by(&self, other: &Node, by: Sorting) -> Ordering {
        if by.dirs_first {
            match (&self.info, &other.info) {
                (NodeInfo::Dir { .. }, NodeInfo::Dir { .. }) => (),
                (NodeInfo::Dir { .. }, _) => return Ordering::Less,
                (_, NodeInfo::Dir { .. }) => return Ordering::Greater,
                _ => (),
            }
        }

        match by.prop {
            SortingProp::None => Ordering::Equal,
            SortingProp::Name => self.file_name().cmp(other.file_name()),
            SortingProp::Size => cmp_in(&self.meta, &other.meta, Metadata::len),
            SortingProp::Extension => match (self.extension(), other.extension()) {
                (Some(a), Some(b)) => a.cmp(b),
                _ => self.file_name().cmp(other.file_name()),
            },
            SortingProp::ATime => cmp_in(&self.meta, &other.meta, |m| m.accessed().unwrap()),
            SortingProp::MTime => cmp_in(&self.meta, &other.meta, |m| m.modified().unwrap()),
            SortingProp::CTime => cmp_in(&self.meta, &other.meta, |m| m.created().unwrap()),
        }
    }

    pub fn loaded_children(&self) -> Option<&Vec<Node>> {
        match &self.info {
            NodeInfo::Dir { loaded, children } if *loaded => Some(children),

            NodeInfo::Link { target: Ok(target) } => target.loaded_children(),

            _ => None,
        }
    }

    pub fn loaded_children_mut(&mut self) -> Option<&mut Vec<Node>> {
        match &mut self.info {
            NodeInfo::Dir { loaded, children } if *loaded => Some(children),

            NodeInfo::Link { target: Ok(target) } => target.loaded_children_mut(),

            _ => None,
        }
    }

    pub fn load_children(&mut self) -> io::Result<&mut Vec<Node>> {
        match &mut self.info {
            NodeInfo::Dir { loaded, children } => {
                if !*loaded {
                    *children = self
                        .path
                        .read_dir()?
                        .filter_map(Result::ok)
                        .map(|ent| ent.metadata().and_then(|meta| Node::new(ent.path(), meta)))
                        .filter_map(Result::ok)
                        .collect();
                    *loaded = true;
                }
                Ok(children)
            }

            NodeInfo::Link { target } => match target {
                Ok(node) => node.load_children(),
                Err(path) => Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!(
                        "(NotADirectory) cannot unfold file at {}",
                        path.to_string_lossy()
                    ),
                )),
            },

            NodeInfo::File { .. } => Err(io::Error::new(
                io::ErrorKind::Other,
                format!(
                    "(NotADirectory) cannot unfold file at {}",
                    self.path.to_string_lossy()
                ),
            )),
        }
    }

    pub fn meta_to_string(&self) -> String {
        match &self.meta {
            Some(meta) => meta_to_string(meta),
            None => "- no meta - ".to_string(),
        }
    }

    pub fn is_dir(&self) -> bool {
        matches!(self.info, NodeInfo::Dir { .. })
    }

    pub fn is_link(&self) -> bool {
        matches!(self.info, NodeInfo::Link { .. })
    }

    pub fn is_file(&self) -> bool {
        matches!(self.info, NodeInfo::File { .. })
    }

    pub fn file_name(&self) -> &str {
        self.path.file_name().unwrap().to_str().unwrap()
    }

    pub fn extension(&self) -> Option<&str> {
        self.path.extension().and_then(|ostr| ostr.to_str())
    }

    pub fn decoration(&self) -> String {
        match &self.info {
            NodeInfo::Dir { .. } => "/".to_string(),
            NodeInfo::Link { target } => match target {
                Ok(node) => format!("@ -> {}{}", node.file_name(), node.decoration()),
                Err(path) => format!("@ ~> {}", path.to_string_lossy()),
            },
            NodeInfo::File { kind } => match kind {
                FileKind::NamedPipe => "|",
                FileKind::Socket => "=",
                FileKind::Executable => "*",
                _ => "",
            }
            .to_string(),
        }
    }

    pub fn style(&self) -> Style {
        let ls_colors = LsColors::from_env().unwrap_or_default();
        ls_colors
            .style_for_path_with_metadata(&self.path, self.meta.as_ref())
            .map(lscolors_to_tui_style)
            .unwrap_or_else(Style::default)
    }
}
