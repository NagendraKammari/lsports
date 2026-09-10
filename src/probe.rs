//! Asking the OS which ports are listening, regardless of who owns them.
//!
//! Socket enumeration only reports sockets belonging to processes we may
//! inspect — on macOS other users' sockets are absent entirely, not merely
//! unattributed. So "nothing is listening on 22" and "something is listening on
//! 22 that you may not look at" are indistinguishable without a second source.
//! Conflating them is the single most misleading thing this tool could do, so we
//! ask a system tool for the full port list and diff against it.

use std::collections::HashSet;
use std::process::Command;

/// Every listening TCP port on the machine. `None` when no strategy worked, in
/// which case callers must stay silent rather than guess.
#[cfg(unix)]
pub fn listening_ports() -> Option<HashSet<u16>> {
    from_ss().or_else(from_netstat)
}

#[cfg(not(unix))]
pub fn listening_ports() -> Option<HashSet<u16>> {
    None
}

/// `ss -Htnl` (Linux): one listening socket per line, local address in column 4.
#[cfg(unix)]
fn from_ss() -> Option<HashSet<u16>> {
    let text = run("ss", &["-Htnl"])?;
    let ports: HashSet<u16> = text
        .lines()
        .filter_map(|line| line.split_whitespace().nth(3).and_then(trailing_port))
        .collect();
    (!ports.is_empty()).then_some(ports)
}

/// `netstat -an -p tcp` (macOS/BSD): every state, local address in column 4.
#[cfg(unix)]
fn from_netstat() -> Option<HashSet<u16>> {
    let text = run("netstat", &["-an", "-p", "tcp"])?;
    let ports: HashSet<u16> = text
        .lines()
        .filter(|line| line.contains("LISTEN"))
        .filter_map(|line| line.split_whitespace().nth(3).and_then(trailing_port))
        .collect();
    (!ports.is_empty()).then_some(ports)
}

#[cfg(unix)]
fn run(bin: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(bin).args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Port from a local-address field. macOS separates it with `.`
/// (`127.0.0.1.6379`, `*.22`), Linux with `:` (`0.0.0.0:22`, `[::]:22`).
#[cfg(unix)]
fn trailing_port(field: &str) -> Option<u16> {
    let tail = field.rsplit(['.', ':']).next()?;
    tail.parse().ok()
}

#[cfg(all(test, unix))]
mod tests {
    use super::trailing_port;

    #[test]
    fn parses_both_platform_formats() {
        assert_eq!(trailing_port("127.0.0.1.6379"), Some(6379));
        assert_eq!(trailing_port("*.22"), Some(22));
        assert_eq!(trailing_port("0.0.0.0:22"), Some(22));
        assert_eq!(trailing_port("[::]:8080"), Some(8080));
        assert_eq!(trailing_port("::1.63330"), Some(63330));
        assert_eq!(trailing_port("*.*"), None);
    }
}
