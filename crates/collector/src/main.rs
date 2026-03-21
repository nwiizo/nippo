mod filter;
mod output;
mod sources;

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

use crate::filter::{DateFilter, Period};
use crate::output::{build_output, format_summary};
use crate::sources::claude_code::{collect_sessions, discover_session_files};

#[derive(Clone, ValueEnum)]
enum OutputFormat {
    /// JSON output (default)
    Json,
    /// Human-readable summary
    Summary,
}

#[derive(Parser)]
#[command(
    name = "nippo",
    version,
    about = "Claude Code session collector for daily reports"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Collect session data from Claude Code JSONL files
    Collect {
        /// Number of days to look back (0 = all time)
        #[arg(long, default_value = "1")]
        days: u32,

        /// Start date (YYYY-MM-DD). Overrides --days
        #[arg(long)]
        from: Option<String>,

        /// End date (YYYY-MM-DD). Defaults to today
        #[arg(long)]
        to: Option<String>,

        /// Named period. Overrides --days
        #[arg(long, value_enum)]
        period: Option<Period>,

        /// Filter by project name (substring match)
        #[arg(long)]
        project: Option<String>,

        /// Output only aggregate statistics
        #[arg(long)]
        stats_only: bool,

        /// Maximum number of sessions to include in output (0 = unlimited)
        #[arg(long, default_value = "0")]
        max_sessions: usize,

        /// Output format
        #[arg(long, value_enum, default_value = "json")]
        format: OutputFormat,

        /// Custom Claude data directory (default: ~/.claude)
        #[arg(long)]
        claude_dir: Option<PathBuf>,
    },
}

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Collect {
            days,
            from,
            to,
            period,
            project,
            stats_only,
            max_sessions,
            format,
            claude_dir,
        } => {
            let claude_dir = claude_dir.unwrap_or_else(|| dirs_home().join(".claude"));

            // Priority: --period > --from/--to > --days
            let filter = if let Some(ref period) = period {
                DateFilter::from_period(period)
            } else if from.is_some() || to.is_some() {
                DateFilter::from_range(from.as_deref(), to.as_deref())?
            } else {
                DateFilter::from_days(days)
            };

            let total_files = discover_session_files(&claude_dir)
                .map(|f| f.len())
                .unwrap_or(0);

            let mut sessions = collect_sessions(&claude_dir, &filter)?;

            if let Some(ref proj) = project {
                let proj_lower = proj.to_lowercase();
                sessions.retain(|s| s.project.to_lowercase().contains(&proj_lower));
            }

            let label = if let Some(ref p) = period {
                period_label(p)
            } else if from.is_some() || to.is_some() {
                format!(
                    "{} ~ {}",
                    from.as_deref().unwrap_or("..."),
                    to.as_deref().unwrap_or("today")
                )
            } else if days == 1 {
                "today".to_string()
            } else if days == 0 {
                "all time".to_string()
            } else {
                format!("{days} days")
            };

            if max_sessions > 0 {
                sessions.truncate(max_sessions);
            }

            let output = build_output(sessions, &label, total_files, stats_only);

            match format {
                OutputFormat::Json => {
                    let json = serde_json::to_string_pretty(&output)?;
                    println!("{json}");
                }
                OutputFormat::Summary => {
                    print!("{}", format_summary(&output));
                }
            }
        }
    }

    Ok(())
}

fn period_label(period: &Period) -> String {
    match period {
        Period::Today => "today".to_string(),
        Period::Yesterday => "yesterday".to_string(),
        Period::ThisWeek => "this week".to_string(),
        Period::LastWeek => "last week".to_string(),
        Period::WeekBeforeLast => "week before last".to_string(),
        Period::ThisMonth => "this month".to_string(),
        Period::LastMonth => "last month".to_string(),
        Period::MonthBeforeLast => "month before last".to_string(),
    }
}

fn dirs_home() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/"))
}
