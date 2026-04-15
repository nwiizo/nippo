mod filter;
mod output;
mod session;
mod sources;

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

use crate::filter::{DateFilter, Period};
use crate::output::{SourceMeta, build_output, format_summary};
use crate::session::RawSession;
use crate::sources::claude_code::{
    collect_sessions as collect_claude_sessions,
    discover_session_files as discover_claude_session_files,
};
use crate::sources::codex::{
    collect_sessions as collect_codex_sessions,
    discover_history_files as discover_codex_history_files,
};

#[derive(Clone, ValueEnum)]
enum OutputFormat {
    /// JSON output (default)
    Json,
    /// Human-readable summary
    Summary,
}

#[derive(Clone, ValueEnum)]
enum DataSource {
    /// Choose the active session source automatically
    Auto,
    /// Read Claude Code session logs
    Claude,
    /// Read Codex history and thread metadata
    Codex,
    /// Merge Claude Code and Codex history
    All,
}

#[derive(Parser)]
#[command(
    name = "nippo",
    version,
    about = "Claude Code / Codex session collector for daily reports",
    long_about = "\
Claude Code / Codex のセッションログを収集・集計するツール。
nippo スキルのデータ収集バックエンドとして動作する。

単体でも使える:
  nippo collect --format summary          今日のサマリー
  nippo collect --days 7 --format summary 過去7日のサマリー
  nippo collect --period last-week        先週のデータ
  nippo collect --project myapp           プロジェクトで絞り込み
  nippo collect --source codex            Codex 履歴のみ

スキルと組み合わせて使う:
  /nippo              日報（事実 + 意思決定 + 用語レビュー）
  /nippo reflection   問いのみ（自分で振り返る）
  /nippo guide        回答 + 学ぶべき概念
  /nippo report       上司・メンター向け進捗報告
  /nippo review       評価面談・自己評価用
  /nippo insight      深い振り返り（ALACT モデル）
  /nippo trend 90     三分割変化分析

https://github.com/nwiizo/nippo",
    after_help = "詳細: https://github.com/nwiizo/nippo"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Collect session data from Claude Code or Codex logs
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

        /// Session source (auto/claude/codex/all)
        #[arg(long, value_enum, default_value = "auto")]
        source: DataSource,

        /// Custom Claude data directory (default: ~/.claude)
        #[arg(long)]
        claude_dir: Option<PathBuf>,

        /// Custom Codex data directory (default: ~/.codex)
        #[arg(long)]
        codex_dir: Option<PathBuf>,
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
            source,
            claude_dir,
            codex_dir,
        } => {
            let home_dir = dirs_home();
            let claude_dir = claude_dir.unwrap_or_else(|| home_dir.join(".claude"));
            let codex_dir = codex_dir.unwrap_or_else(|| home_dir.join(".codex"));

            // Priority: --period > --from/--to > --days
            let filter = if let Some(ref period) = period {
                DateFilter::from_period(period)
            } else if from.is_some() || to.is_some() {
                DateFilter::from_range(from.as_deref(), to.as_deref())?
            } else {
                DateFilter::from_days(days)
            };

            let selected_sources = resolve_sources(&source, &claude_dir, &codex_dir);
            let (mut sessions, total_files) =
                collect_from_sources(&selected_sources, &claude_dir, &codex_dir, &filter)?;

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

            let output = build_output(
                sessions,
                &label,
                total_files,
                stats_only,
                SourceMeta {
                    requested: source_name(&source).to_string(),
                    resolved: selected_sources
                        .iter()
                        .map(source_name)
                        .map(str::to_string)
                        .collect(),
                },
            );

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

fn resolve_sources(
    source: &DataSource,
    claude_dir: &std::path::Path,
    codex_dir: &std::path::Path,
) -> Vec<DataSource> {
    match source {
        DataSource::Auto => vec![detect_auto_source(claude_dir, codex_dir)],
        DataSource::Claude => vec![DataSource::Claude],
        DataSource::Codex => vec![DataSource::Codex],
        DataSource::All => {
            let mut sources = Vec::new();
            if claude_available(claude_dir) {
                sources.push(DataSource::Claude);
            }
            if codex_available(codex_dir) {
                sources.push(DataSource::Codex);
            }
            if sources.is_empty() {
                sources.push(detect_auto_source(claude_dir, codex_dir));
            }
            sources
        }
    }
}

fn detect_auto_source(claude_dir: &std::path::Path, codex_dir: &std::path::Path) -> DataSource {
    if std::env::var_os("CODEX_THREAD_ID").is_some() && codex_available(codex_dir) {
        return DataSource::Codex;
    }
    if claude_available(claude_dir) {
        return DataSource::Claude;
    }
    if codex_available(codex_dir) {
        return DataSource::Codex;
    }
    DataSource::Claude
}

fn collect_from_sources(
    sources: &[DataSource],
    claude_dir: &std::path::Path,
    codex_dir: &std::path::Path,
    filter: &DateFilter,
) -> Result<(Vec<RawSession>, usize)> {
    let mut sessions = Vec::new();
    let mut total_files = 0;

    for source in sources {
        match source {
            DataSource::Claude => {
                total_files += discover_claude_session_files(claude_dir)?.len();
                sessions.extend(collect_claude_sessions(claude_dir, filter)?);
            }
            DataSource::Codex => {
                total_files += discover_codex_history_files(codex_dir)?.len();
                sessions.extend(collect_codex_sessions(codex_dir, filter)?);
            }
            DataSource::Auto | DataSource::All => unreachable!("source must be resolved first"),
        }
    }

    Ok((sessions, total_files))
}

fn claude_available(claude_dir: &std::path::Path) -> bool {
    claude_dir.join("projects").exists()
}

fn codex_available(codex_dir: &std::path::Path) -> bool {
    codex_dir.join("history.jsonl").exists()
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

fn source_name(source: &DataSource) -> &'static str {
    match source {
        DataSource::Auto => "auto",
        DataSource::Claude => "claude",
        DataSource::Codex => "codex",
        DataSource::All => "all",
    }
}

fn dirs_home() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/"))
}
