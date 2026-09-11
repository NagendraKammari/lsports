use serde::Serialize;
use std::net::IpAddr;
use std::path::PathBuf;

/// How reachable a listening socket is. Surfaced because a dev server bound to
/// all interfaces is exposed to everyone on the local network, and people
/// routinely do this by accident.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reach {
    /// Bound to 127.0.0.1 / ::1 — this machine only.
    Loopback,
    /// Bound to 0.0.0.0 / :: — reachable from the network.
    AllInterfaces,
    /// Bound to one specific non-loopback address.
    Address,
}

#[derive(Debug, Clone, Serialize)]
pub struct GitInfo {
    pub root: PathBuf,
    pub branch: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Ancestor {
    pub pid: u32,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    pub port: u16,
    pub addr: IpAddr,
    pub reach: Reach,
    pub protocol: &'static str,
    pub pid: u32,
    pub name: String,
    pub exe: Option<PathBuf>,
    pub cmd: Option<String>,
    pub cwd: Option<PathBuf>,
    /// Seconds since the process started. `None` when the kernel would not say.
    pub uptime_secs: Option<u64>,
    pub git: Option<GitInfo>,
    /// Nearest ancestors, closest first. Empty when the process was reparented
    /// to init/launchd, which is the common case for daemons.
    pub ancestry: Vec<Ancestor>,
    /// Whether the process runs as the current user. `None` when the platform
    /// would not say. `free` refuses to signal anything that is a definite
    /// `false`, so an unknown must never be treated as a denial.
    pub same_user: Option<bool>,
}

/// A whole-run result, including what we could *not* see.
#[derive(Debug, Serialize)]
pub struct Report {
    pub entries: Vec<Entry>,
    /// Total listening sockets on the machine, including those owned by
    /// processes we lack the privilege to inspect. Only meaningful for an
    /// unfiltered run; `None` otherwise, since a machine-wide count would be a
    /// non-answer to a question about one port.
    pub total_listening: Option<usize>,
    /// Requested ports that *are* listening but whose owner we cannot see.
    /// This is the useful answer to "what's on 6379?" when the owner is root.
    pub blocked_ports: Vec<u16>,
    /// The ports asked about, empty for a survey of everything.
    pub requested: Vec<u16>,
}
