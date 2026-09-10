mod collect;
mod color;
mod git;
mod model;
mod probe;
mod render;

use anyhow::Result;
use clap::Parser;
use std::io::IsTerminal;

#[derive(Parser)]
#[command(
    name = "lsports",
    version,
    about = "See what's listening on your ports — and why it's there.",
    long_about = "Lists listening TCP ports with the context that identifies them: \
                  working directory, git branch, uptime, and how the process was launched."
)]
struct Cli {
    /// Ports to inspect, e.g. `3000 8080`. Omit to list everything.
    ports: Vec<u16>,

    /// Machine-readable output.
    #[arg(long)]
    json: bool,

    /// Full detail for every row, not just filtered ones.
    #[arg(short, long)]
    long: bool,

    /// Never emit colour (also honours the NO_COLOR environment variable).
    #[arg(long)]
    no_color: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let use_color = !cli.no_color
        && !cli.json
        && std::env::var_os("NO_COLOR").is_none()
        && std::io::stdout().is_terminal();
    color::set_enabled(use_color);

    let report = collect::collect(&cli.ports)?;

    // One port asked about is a question about that process; a bare invocation
    // is a survey. Match the output shape to the question.
    let detailed = cli.long || cli.ports.len() == 1;

    let text = if cli.json {
        render::json(&report)?
    } else if detailed {
        render::detail(&report)
    } else {
        render::table(&report)
    };
    print!("{text}");

    Ok(())
}
