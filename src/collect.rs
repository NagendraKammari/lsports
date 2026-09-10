use crate::git;
use crate::model::{Ancestor, Entry, Reach, Report};
use crate::probe;
use anyhow::{Context, Result};
use netstat2::{AddressFamilyFlags, ProtocolFlags, ProtocolSocketInfo, TcpState};
use std::collections::HashMap;
use std::net::IpAddr;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

/// Deep enough to show "npm run dev ← zsh" without printing the whole session.
const MAX_ANCESTORS: usize = 4;

pub fn collect(filter: &[u16]) -> Result<Report> {
    let sockets = netstat2::get_sockets_info(
        AddressFamilyFlags::IPV4 | AddressFamilyFlags::IPV6,
        ProtocolFlags::TCP,
    )
    .context("could not enumerate sockets")?;

    // Only the listening processes and their ancestors matter, and refreshing
    // every process on the machine to find a dozen of them dominates runtime.
    let listeners: Vec<Pid> = sockets
        .iter()
        .filter(|si| {
            matches!(&si.protocol_socket_info,
            ProtocolSocketInfo::Tcp(tcp) if tcp.state == TcpState::Listen)
        })
        .flat_map(|si| si.associated_pids.iter().copied())
        .map(Pid::from_u32)
        .collect();

    let mut sys = System::new();
    let detail = ProcessRefreshKind::nothing()
        .with_cwd(UpdateKind::Always)
        .with_cmd(UpdateKind::Always)
        .with_exe(UpdateKind::Always);
    refresh(&mut sys, &listeners, detail);
    refresh_ancestors(&mut sys, &listeners, detail);

    // Keyed on (pid, port) so a process listening on both IPv4 and IPv6 is one
    // row. Where the two differ, the wider binding wins — being reachable from
    // the network is the fact worth reporting.
    let mut best: HashMap<(u32, u16), Entry> = HashMap::new();
    let mut unattributed = 0usize;
    let mut blocked_ports = Vec::new();

    for si in sockets {
        let ProtocolSocketInfo::Tcp(tcp) = &si.protocol_socket_info else {
            continue;
        };
        if tcp.state != TcpState::Listen {
            continue;
        }

        // Attribution is attempted for every socket, not just the requested
        // ones, so the machine-wide count stays correct under a filter.
        let wanted = filter.is_empty() || filter.contains(&tcp.local_port);
        let proc = si
            .associated_pids
            .first()
            .and_then(|&pid| sys.process(Pid::from_u32(pid)).map(|p| (pid, p)));

        let Some((pid_raw, proc)) = proc else {
            unattributed += 1;
            if wanted && !blocked_ports.contains(&tcp.local_port) {
                blocked_ports.push(tcp.local_port);
            }
            continue;
        };
        if !wanted {
            continue;
        }
        let pid = Pid::from_u32(pid_raw);

        let cwd = proc.cwd().map(|p| p.to_path_buf());
        let git = cwd.as_deref().and_then(git::detect);
        let addr = normalize(tcp.local_addr);
        let candidate = Entry {
            port: tcp.local_port,
            addr,
            reach: reach_of(addr),
            protocol: "tcp",
            pid: pid_raw,
            name: proc.name().to_string_lossy().to_string(),
            exe: proc.exe().map(|p| p.to_path_buf()),
            cmd: Some(join_cmd(proc.cmd())),
            cwd,
            uptime_secs: Some(proc.run_time()),
            git,
            ancestry: ancestry(&sys, pid),
        };

        best.entry((pid_raw, tcp.local_port))
            .and_modify(|existing| {
                if wider(candidate.reach, existing.reach) {
                    existing.reach = candidate.reach;
                    existing.addr = candidate.addr;
                }
            })
            .or_insert(candidate);
    }

    let mut entries: Vec<Entry> = best.into_values().collect();
    entries.sort_by(|a, b| a.port.cmp(&b.port).then(a.pid.cmp(&b.pid)));

    // Under a filter we must know whether a requested port is genuinely free or
    // merely opaque to us. Unfiltered, we only need the machine-wide total, and
    // then only if socket enumeration did not already tell us.
    let mut total_listening = None;
    let need_probe = !filter.is_empty() || unattributed == 0;
    if need_probe {
        if let Some(all_ports) = probe::listening_ports() {
            if filter.is_empty() {
                total_listening = Some(all_ports.len().max(entries.len()));
            } else {
                for port in filter {
                    let seen = entries.iter().any(|e| e.port == *port);
                    if all_ports.contains(port) && !seen && !blocked_ports.contains(port) {
                        blocked_ports.push(*port);
                    }
                }
            }
        }
    }
    if filter.is_empty() && unattributed > 0 {
        total_listening = Some(entries.len() + unattributed);
    }

    blocked_ports.sort_unstable();

    Ok(Report {
        entries,
        total_listening,
        blocked_ports,
        requested: filter.to_vec(),
    })
}

/// Load just these processes. `remove_dead_processes` stays false because these
/// are partial refreshes — a process absent from `pids` is not dead, it is
/// simply not being asked about, and dropping it would discard ancestors
/// gathered by an earlier pass.
fn refresh(sys: &mut System, pids: &[Pid], kind: ProcessRefreshKind) {
    if pids.is_empty() {
        return;
    }
    sys.refresh_processes_specifics(ProcessesToUpdate::Some(pids), false, kind);
}

/// Walk up from `seed`, loading each generation, so the launch chain can be
/// read without refreshing the whole process table.
fn refresh_ancestors(sys: &mut System, seed: &[Pid], kind: ProcessRefreshKind) {
    let mut frontier: Vec<Pid> = seed.to_vec();
    for _ in 0..MAX_ANCESTORS {
        let parents: Vec<Pid> = frontier
            .iter()
            .filter_map(|pid| sys.process(*pid)?.parent())
            .filter(|parent| parent.as_u32() > 1 && sys.process(*parent).is_none())
            .collect();
        if parents.is_empty() {
            return;
        }
        refresh(sys, &parents, kind);
        frontier = parents;
    }
}

/// Collapse IPv4-in-IPv6 forms to plain IPv4. Sockets are reported as e.g.
/// `::7f00:1`, which is 127.0.0.1 written the deprecated IPv4-compatible way —
/// left alone it reads as a routable address and gets misclassified.
///
/// `Ipv6Addr::to_ipv4` would turn `::1` into `0.0.0.1`, since `::1` is itself an
/// IPv4-compatible address, so loopback and unspecified are settled first.
fn normalize(ip: IpAddr) -> IpAddr {
    let IpAddr::V6(v6) = ip else { return ip };
    if v6.is_loopback() || v6.is_unspecified() {
        return ip;
    }
    match v6.to_ipv4() {
        Some(v4) if !v4.is_unspecified() => IpAddr::V4(v4),
        _ => ip,
    }
}

fn reach_of(ip: IpAddr) -> Reach {
    if ip.is_loopback() {
        Reach::Loopback
    } else if ip.is_unspecified() {
        Reach::AllInterfaces
    } else {
        Reach::Address
    }
}

fn wider(a: Reach, b: Reach) -> bool {
    fn rank(r: Reach) -> u8 {
        match r {
            Reach::Loopback => 0,
            Reach::Address => 1,
            Reach::AllInterfaces => 2,
        }
    }
    rank(a) > rank(b)
}

fn join_cmd(cmd: &[std::ffi::OsString]) -> String {
    cmd.iter()
        .map(|s| s.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Ancestors nearest-first. Returns empty when the process was reparented to
/// init/launchd, since "parent: launchd" tells the reader nothing.
fn ancestry(sys: &System, start: Pid) -> Vec<Ancestor> {
    let mut out = Vec::new();
    let mut cursor = start;
    while out.len() < MAX_ANCESTORS {
        let Some(parent) = sys.process(cursor).and_then(|p| p.parent()) else {
            break;
        };
        if parent.as_u32() <= 1 {
            break;
        }
        let Some(proc) = sys.process(parent) else {
            break;
        };
        out.push(Ancestor {
            pid: parent.as_u32(),
            name: proc.name().to_string_lossy().to_string(),
        });
        cursor = parent;
    }
    out
}
