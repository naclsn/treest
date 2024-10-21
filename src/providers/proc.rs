use std::cmp::Ordering;
use std::fmt::{Display, Formatter, Result as FmtResult};

use anyhow::Result;
use sysinfo::{Pid, ProcessRefreshKind, RefreshKind, System, UpdateKind};

use crate::fisovec::FilterSorter;
use crate::tree::{Provider, ProviderExt};

pub struct Proc {
    system: Box<System>,
}

#[derive(Clone)]
pub struct ProcInfo {
    pid: Pid,
    name: String,
    cpids: Vec<Pid>,
}
pub enum ProcNode {
    Roots(Vec<ProcInfo>),
    Info(ProcInfo),
    Command,
    CmdArg(usize, String),
    Environ,
    EnvVar(String),
}
use ProcNode::*;

impl PartialEq for ProcInfo {
    fn eq(&self, other: &Self) -> bool {
        self.pid == other.pid
    }
}
impl PartialEq for ProcNode {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Roots(_), Roots(_)) => true,
            (Info(l), Info(r)) => l == r,
            _ => false,
        }
    }
}

impl Display for ProcNode {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Roots(_) => write!(f, "system\x1b[m "),

            Info(ProcInfo { pid, name, .. }) => write!(f, "({:6}) {name}\x1b[m ", pid.as_u32()),

            Command => write!(f, "\x1b[34mcommand\x1b[m "),

            CmdArg(_, s) => write!(f, "{s}"),

            Environ => write!(f, "\x1b[34menviron\x1b[m "),

            EnvVar(s) => {
                let eq = s.find('=').unwrap_or(s.len() - 1);
                write!(
                    f,
                    "\x1b[36m{}\x1b[m=\x1b[32m{}\x1b[m",
                    &s[..eq],
                    &s[eq + 1..std::cmp::max(eq + 1, std::cmp::min(s.len(), 42))]
                )
            }
        }
    }
}

impl Provider for Proc {
    type Fragment = ProcNode;

    fn provide_root(&self) -> Self::Fragment {
        let roots = self
            .system
            .processes()
            .iter()
            .filter_map(|(&pid, proc)| {
                if proc.parent().is_none() {
                    Some(self.info_for_pid(pid))
                } else {
                    None
                }
            })
            .collect();
        Roots(roots)
    }

    fn provide(&mut self, path: &[&Self::Fragment]) -> Vec<Self::Fragment> {
        match path {
            [Roots(roots)] => roots.iter().map(|info| Info(info.clone())).collect(),

            [.., Info(ProcInfo { cpids, .. })] => {
                let mut r = vec![Command, Environ];
                r.extend(cpids.iter().map(|&pid| Info(self.info_for_pid(pid))));
                r
            }

            [.., Info(ProcInfo { pid, .. }), Command] => self
                .system
                .process(*pid)
                .unwrap()
                .cmd()
                .iter()
                .enumerate()
                .map(|(k, arg)| CmdArg(k, arg.to_string_lossy().to_string()))
                .collect(),

            [.., CmdArg(_, _)] => Vec::new(),

            [.., Info(ProcInfo { pid, .. }), Environ] => self
                .system
                .process(*pid)
                .unwrap()
                .environ()
                .iter()
                .map(|osstr| EnvVar(osstr.to_string_lossy().to_string()))
                .collect(),

            [.., EnvVar(_)] => Vec::new(),

            _ => unreachable!(),
        }
    }
}

impl ProviderExt for Proc {
    fn write_nav_path(&self, f: &mut impl std::fmt::Write, path: &[&Self::Fragment]) -> FmtResult {
        match path {
            [Roots(_)] => {
                let n = self.system.processes().len();
                write!(f, "({} process{})", n, if 1 == n { " (!?)" } else { "es" })
            }

            [.., Info(ProcInfo { pid, name, .. })] => {
                let proc = self.system.process(*pid).unwrap();
                write!(f, "({:6}) cwd:", pid.as_u32())?;
                proc.cwd()
                    .map(|path| write!(f, "{}", path.display()))
                    .unwrap_or_else(|| write!(f, "/"))?;
                write!(f, " exe:")?;
                proc.exe()
                    .map(|path| write!(f, "{}", path.display()))
                    .unwrap_or_else(|| write!(f, "<{name}>"))
            }

            [.., Info(ProcInfo { pid, .. }), Command] => {
                let n = self.system.process(*pid).unwrap().cmd().len();
                write!(f, "({} argument{})", n, if 1 == n { "" } else { "s" })
            }

            [.., CmdArg(_, s)] => write!(f, "{s}"),

            [.., Info(ProcInfo { pid, .. }), Environ] => {
                let n = self.system.process(*pid).unwrap().environ().len();
                write!(f, "({} variable{})", n, if 1 == n { "" } else { "s" })
            }

            [.., EnvVar(s)] => {
                let eq = s.find('=').unwrap_or(s.len() - 1);
                write!(
                    f,
                    "\x1b[36m{}\x1b[m=\x1b[32m{}\x1b[m",
                    &s[..eq],
                    &s[eq + 1..]
                )
            }

            _ => unreachable!(),
        }
    }
}

impl FilterSorter<ProcNode> for Proc {
    fn compare(&self, a: &ProcNode, b: &ProcNode) -> Option<Ordering> {
        match (a, b) {
            (Info(a), Info(b)) => {
                Some(Ord::cmp(&a.pid, &b.pid))
                //Option::zip(self.system.0.process(a.pid), self.system.0.process(b.pid))
                //    .map(|(a, b)| Ord::cmp(a.name(), b.name()))
            }

            (Command, _) => Some(Ordering::Less),
            (_, Command) => Some(Ordering::Greater),

            (CmdArg(a, _), CmdArg(b, _)) => Some(Ord::cmp(a, b)),

            (Environ, _) => Some(Ordering::Less),
            (_, Environ) => Some(Ordering::Greater),

            (EnvVar(a), EnvVar(b)) => Some(Ord::cmp(a, b)),

            _ => unreachable!(),
        }
    }

    fn keep(&self, _a: &ProcNode) -> bool {
        true
    }
}

impl Proc {
    fn refresh_kind() -> RefreshKind {
        RefreshKind::new().with_processes(
            ProcessRefreshKind::new()
                //.with_user(UpdateKind::OnlyIfNotSet)
                .with_cwd(UpdateKind::OnlyIfNotSet)
                .with_environ(UpdateKind::OnlyIfNotSet)
                .with_cmd(UpdateKind::OnlyIfNotSet)
                .with_exe(UpdateKind::OnlyIfNotSet),
        )
    }

    fn info_for_pid(&self, pid: Pid) -> ProcInfo {
        ProcInfo {
            pid,
            name: self
                .system
                .process(pid)
                .unwrap()
                .name()
                .to_string_lossy()
                .to_string(),
            cpids: self
                .system
                .processes()
                .iter()
                .filter_map(|(&cpid, proc)| {
                    if proc.parent() == Some(pid) {
                        Some(cpid)
                    } else {
                        None
                    }
                })
                .collect(),
        }
    }

    pub fn new(_: &str) -> Result<Self> {
        Ok(Proc {
            system: Box::new(System::new_with_specifics(Proc::refresh_kind())),
        })
    }
}
