use crate::color::{bold, cyan, dim, green, yellow};
use crate::model::{Entry, Reach, Report};
use std::path::Path;

pub fn json(report: &Report) -> anyhow::Result<String> {
    Ok(serde_json::to_string_pretty(report)?)
}

pub fn table(report: &Report) -> String {
    let mut out = String::new();

    if report.entries.is_empty() {
        out.push_str(&dim(&format!("{}\n", empty_message(report))));
    } else {
        let rows: Vec<(String, String, String, String)> = report
            .entries
            .iter()
            .map(|e| {
                (
                    format!(":{}", e.port),
                    label(e),
                    where_of(e),
                    e.uptime_secs.map(duration).unwrap_or_else(|| "—".into()),
                )
            })
            .collect();

        let w_port = rows.iter().map(|r| r.0.len()).max().unwrap_or(0);
        let w_proc = rows.iter().map(|r| r.1.len()).max().unwrap_or(0).min(34);
        let w_where = rows.iter().map(|r| r.2.len()).max().unwrap_or(0).min(40);

        for (row, entry) in rows.iter().zip(&report.entries) {
            let flag = if entry.reach == Reach::AllInterfaces {
                yellow(" ⚠ exposed")
            } else {
                String::new()
            };
            out.push_str(&format!(
                "{:<w_port$}  {:<w_proc$}  {:<w_where$}  {}{}\n",
                cyan(&row.0),
                bold(&clip(&row.1, w_proc)),
                dim(&clip(&row.2, w_where)),
                dim(&format!("up {}", row.3)),
                flag,
                // Widths are padded against the *styled* strings, so add the
                // invisible escape bytes back in to keep columns aligned.
                w_port = w_port + invisible(&cyan("")),
                w_proc = w_proc + invisible(&bold("")),
                w_where = w_where + invisible(&dim("")),
            ));
        }
    }

    for note in notes(report) {
        out.push_str(&dim(&format!("\n{note}\n")));
    }
    out
}

pub fn detail(report: &Report) -> String {
    let mut out = String::new();

    // Saying "nothing is listening" alongside "port 22 is listening" would be a
    // contradiction; the blocked-port note below is the more precise answer.
    if report.entries.is_empty() && report.blocked_ports.is_empty() {
        out.push_str(&dim(&format!("{}\n", empty_message(report))));
    }

    for e in &report.entries {
        out.push_str(&format!(
            "{}  {}\n",
            cyan(&bold(&format!(":{}", e.port))),
            bold(&label(e))
        ));

        let mut facts = vec![format!("pid {}", e.pid)];
        if let Some(secs) = e.uptime_secs {
            facts.push(format!("up {}", duration(secs)));
        }
        facts.push(reach_text(e));
        out.push_str(&format!("       {}\n", dim(&facts.join(" · "))));

        if let Some(cwd) = useful_cwd(e) {
            let mut line = format!("cwd  {}", tildify(cwd));
            if let Some(git) = &e.git {
                line.push_str(&format!("  ({})", green(&format!("git: {}", git.branch))));
            }
            out.push_str(&format!("       {line}\n"));
        }

        if !e.ancestry.is_empty() {
            let chain = e
                .ancestry
                .iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>()
                .join(" ← ");
            out.push_str(&format!("       via  {}\n", dim(&chain)));
        }

        if let Some(cmd) = &e.cmd {
            if !cmd.is_empty() {
                out.push_str(&format!("       cmd  {}\n", dim(&clip(cmd, 78))));
            }
        }
        out.push('\n');
    }

    for note in notes(report) {
        out.push_str(&dim(&format!("{note}\n")));
    }
    out
}

/// The process name, plus a hint when the raw name is uninformative. `java`
/// listening from a Gradle daemon directory is the common confusing case.
fn label(e: &Entry) -> String {
    let cwd = e.cwd.as_ref().map(|p| p.to_string_lossy().to_string());
    if let Some(cwd) = cwd {
        if cwd.contains("/.gradle/daemon") {
            return format!("{} (gradle daemon)", e.name);
        }
        if cwd.contains("/homebrew") {
            return format!("{} (homebrew)", e.name);
        }
    }
    e.name.clone()
}

fn where_of(e: &Entry) -> String {
    match (&e.git, useful_cwd(e)) {
        (Some(git), _) => format!("{} ({})", tildify(&git.root), git.branch),
        (None, Some(cwd)) => tildify(cwd),
        (None, None) => "—".into(),
    }
}

/// A process whose working directory is `/` or `$HOME` has not told us anything
/// — those are inherited defaults, not a location the process cares about.
/// Reported verbatim in `--json`, but suppressed on screen as pure noise.
fn useful_cwd(e: &Entry) -> Option<&Path> {
    let cwd = e.cwd.as_deref()?;
    cwd.parent()?;
    if let Ok(home) = std::env::var("HOME") {
        if !home.is_empty() && cwd == Path::new(&home) {
            return None;
        }
    }
    Some(cwd)
}

fn reach_text(e: &Entry) -> String {
    match e.reach {
        Reach::Loopback => format!("{} only", e.addr),
        Reach::AllInterfaces => "all interfaces — reachable from your network".into(),
        Reach::Address => format!("on {}", e.addr),
    }
}

fn empty_message(report: &Report) -> String {
    match report.requested.as_slice() {
        [] => "no listening tcp ports visible to you".into(),
        [port] => format!("nothing is listening on port {port}"),
        _ => "nothing is listening on those ports".into(),
    }
}

/// Notes about what we could not see. Under a port filter this names the
/// specific ports; unfiltered it reports the machine-wide gap.
fn notes(report: &Report) -> Vec<String> {
    let mut out = Vec::new();

    if !report.blocked_ports.is_empty() {
        let list = report
            .blocked_ports
            .iter()
            .map(|p| p.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let (subject, verb) = if report.blocked_ports.len() == 1 {
            ("port", "is")
        } else {
            ("ports", "are")
        };
        out.push(format!(
            "{subject} {list} {verb} listening, but owned by another user — re-run with sudo to identify"
        ));
    }

    if let Some(total) = report.total_listening {
        let unseen = total.saturating_sub(report.entries.len());
        if unseen > 0 {
            out.push(format!(
                "{unseen} more listening socket(s) belong to other users — re-run with sudo to identify them"
            ));
        }
    }
    out
}

fn tildify(path: &Path) -> String {
    let s = path.to_string_lossy().to_string();
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() && s.starts_with(&home) => s.replacen(&home, "~", 1),
        _ => s,
    }
}

fn duration(secs: u64) -> String {
    match secs {
        s if s < 60 => format!("{s}s"),
        s if s < 3_600 => format!("{}m", s / 60),
        s if s < 86_400 => format!("{}h{:02}m", s / 3_600, (s % 3_600) / 60),
        s => format!("{}d{}h", s / 86_400, (s % 86_400) / 3_600),
    }
}

fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let kept: String = text.chars().take(max.saturating_sub(1)).collect();
    format!("{kept}…")
}

/// Byte length of the ANSI escapes in a styled empty string, so format! width
/// specifiers can be corrected for characters that occupy no columns.
fn invisible(styled_empty: &str) -> usize {
    styled_empty.len()
}
