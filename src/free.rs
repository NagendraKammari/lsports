//! Releasing a port by terminating whatever holds it.
//!
//! This is the one part of the tool that changes the state of the machine, so
//! it is deliberately conservative: it shows what it found before doing
//! anything, asks first, sends SIGTERM before SIGKILL, and refuses outright to
//! signal a process belonging to another user.

use crate::collect;
use crate::color::{bold, dim, yellow};
use crate::model::Entry;
use crate::render;
use anyhow::{bail, Result};
use std::io::{IsTerminal, Write};
use std::time::{Duration, Instant};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, Signal, System};

/// How often to re-check whether a terminated process has actually gone.
const POLL: Duration = Duration::from_millis(100);

pub struct Options {
    pub yes: bool,
    pub dry_run: bool,
    pub grace: u64,
}

/// Exit code 1 signals "the port is still in use", so scripts can branch on it.
pub fn run(ports: &[u16], opts: &Options) -> Result<bool> {
    let report = collect::collect(ports)?;

    if !report.blocked_ports.is_empty() {
        let list = join(&report.blocked_ports);
        bail!(
            "port {list} is held by another user's process, which cannot be \
             signalled without sudo — and running this tool as root to \
             terminate someone else's process is not something it will help with"
        );
    }

    // Nothing to do is a success: `lsports free 3000` should be safe to call
    // unconditionally in a script without guarding it first.
    if report.entries.is_empty() {
        println!(
            "{}",
            dim(&format!("nothing to free on port {}", join(ports)))
        );
        return Ok(true);
    }

    let (mine, theirs): (Vec<&Entry>, Vec<&Entry>) = report
        .entries
        .iter()
        .partition(|e| e.same_user != Some(false));
    for e in &theirs {
        eprintln!(
            "{} :{} is {} (pid {}), which runs as another user — skipping",
            yellow("skip"),
            e.port,
            e.name,
            e.pid
        );
    }
    if mine.is_empty() {
        bail!("nothing on port {} belongs to you", join(ports));
    }

    print!("{}", render::detail_of(&mine));

    if opts.dry_run {
        println!(
            "{}",
            dim(&format!(
                "dry run — would send SIGTERM to {}",
                join_pids(&mine)
            ))
        );
        return Ok(true);
    }

    if !opts.yes && !confirm(&format!("Terminate {}?", describe(&mine)))? {
        println!("{}", dim("left alone"));
        return Ok(false);
    }

    let pids: Vec<Pid> = mine.iter().map(|e| Pid::from_u32(e.pid)).collect();
    let mut sys = System::new();
    signal(&mut sys, &pids, Signal::Term);

    let alive = wait_for_exit(&mut sys, &pids, Duration::from_secs(opts.grace));
    if alive.is_empty() {
        println!("{}", bold(&format!("freed :{}", join(ports))));
        return Ok(true);
    }

    let survivors = describe_pids(&alive);
    eprintln!(
        "{}",
        yellow(&format!(
            "{survivors} ignored SIGTERM after {}s",
            opts.grace
        ))
    );

    // --yes means "get this port back", so escalation is implied rather than
    // requiring a second confirmation a script cannot answer.
    if !opts.yes && !confirm("Force kill with SIGKILL?")? {
        println!("{}", dim("left running"));
        return Ok(false);
    }

    signal(&mut sys, &alive, Signal::Kill);
    let stubborn = wait_for_exit(&mut sys, &alive, Duration::from_secs(2));
    if stubborn.is_empty() {
        println!("{}", bold(&format!("freed :{} (forced)", join(ports))));
        Ok(true)
    } else {
        eprintln!(
            "could not terminate {} — it may be unkillable (stuck in a syscall) \
             or protected by the system",
            describe_pids(&stubborn)
        );
        Ok(false)
    }
}

fn signal(sys: &mut System, pids: &[Pid], sig: Signal) {
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(pids),
        false,
        ProcessRefreshKind::nothing(),
    );
    for pid in pids {
        if let Some(proc) = sys.process(*pid) {
            proc.kill_with(sig);
        }
    }
}

/// Poll until every pid is gone or `budget` elapses; returns the survivors.
fn wait_for_exit(sys: &mut System, pids: &[Pid], budget: Duration) -> Vec<Pid> {
    let deadline = Instant::now() + budget;
    loop {
        // remove_dead_processes is true here precisely so a process that has
        // exited disappears from the snapshot rather than lingering.
        sys.refresh_processes_specifics(
            ProcessesToUpdate::Some(pids),
            true,
            ProcessRefreshKind::nothing(),
        );
        let alive: Vec<Pid> = pids
            .iter()
            .copied()
            .filter(|pid| sys.process(*pid).is_some())
            .collect();
        if alive.is_empty() || Instant::now() >= deadline {
            return alive;
        }
        std::thread::sleep(POLL);
    }
}

fn confirm(question: &str) -> Result<bool> {
    if !std::io::stdin().is_terminal() {
        bail!("{question} — refusing to guess with no terminal to ask; pass --yes");
    }
    print!("{question} [y/N] ");
    std::io::stdout().flush()?;
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes"))
}

fn describe(entries: &[&Entry]) -> String {
    match entries {
        [one] => format!("{} (pid {})", one.name, one.pid),
        many => format!("{} processes", many.len()),
    }
}

fn describe_pids(pids: &[Pid]) -> String {
    match pids {
        [one] => format!("pid {one}"),
        many => format!("{} processes", many.len()),
    }
}

fn join(ports: &[u16]) -> String {
    ports
        .iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn join_pids(entries: &[&Entry]) -> String {
    entries
        .iter()
        .map(|e| format!("pid {}", e.pid))
        .collect::<Vec<_>>()
        .join(", ")
}
