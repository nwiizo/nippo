mod filter;
mod ledger;
#[cfg(feature = "tui")]
mod ledger_tui;
mod output;
mod session;
mod skill_install;
mod sources;

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::filter::{DateFilter, Period, local_timezone_name};
use crate::output::{OutputPeriod, SourceMeta, build_output, format_summary};
use crate::session::{
    RawSession, merge_sessions_by_id, retain_meaningful_prompts, sort_sessions_by_recency,
};
use crate::skill_install::{SkillTarget, install as install_skill};
use crate::sources::claude_code::{
    collect_sessions as collect_claude_sessions,
    discover_session_files as discover_claude_session_files,
};
use crate::sources::codex::{
    collect_sessions as collect_codex_sessions,
    discover_history_files as discover_codex_history_files,
};
use crate::sources::opencode::{
    collect_sessions as collect_opencode_sessions,
    discover_history_files as discover_opencode_history_files,
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
    /// Read opencode SQLite history (~/.local/share/opencode)
    Opencode,
    /// Merge Claude Code, Codex, and opencode history
    All,
}

#[derive(Subcommand)]
enum SkillAction {
    /// Install the nippo skill for Claude Code, Codex, or both
    Install {
        /// Skill host to install for
        #[arg(long, value_enum, default_value = "all")]
        target: SkillTarget,

        /// Replace an existing file, directory, or unexpected symlink
        #[arg(long)]
        force: bool,
    },
}

#[derive(Parser)]
#[command(
    name = "nippo",
    version,
    about = "Claude Code / Codex / opencode session collector for daily reports",
    long_about = "\
Claude Code / Codex / opencode のセッションログを収集・集計するツール。
nippo スキルのデータ収集バックエンドとして動作する。

単体でも使える:
  nippo collect --format summary          今日のサマリー
  nippo collect --days 7 --format summary 過去7日のサマリー
  nippo collect --period last-week        先週のデータ
  nippo collect --project myapp           プロジェクトで絞り込み
  nippo collect --source codex            Codex 履歴のみ
  nippo collect --include-prompt-noise --include-self
                                          除外前の記録を確認

スキルをセットアップする:
  nippo skill install                     Claude Code + Codex

スキルと組み合わせて使う:
  /nippo              日報（事実 + 意思決定 + 用語レビュー）
  /nippo reflection   問いのみ（自分で振り返る）
  /nippo plan         前日の振り返りから今日の実験候補を提示
  /nippo guide        回答 + 学ぶべき概念
  /nippo report       上司・メンター向け進捗報告
  /nippo review       評価面談・自己評価用
  /nippo insight      深い振り返り（ALACT モデル）
  /nippo trend 90     三分割変化分析
  /nippo ledger       詰まりの累積集計と収束/発散判定

https://github.com/nwiizo/nippo",
    after_help = "詳細: https://github.com/nwiizo/nippo"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Collect session data from Claude Code, Codex, or opencode logs
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

        /// Include deterministic command, harness, image, interrupt, and acknowledgement noise
        #[arg(long)]
        include_prompt_noise: bool,

        /// Include the Claude Code or Codex session running this command
        #[arg(long)]
        include_self: bool,

        /// Maximum number of sessions to include in output (0 = unlimited)
        #[arg(long, default_value = "0")]
        max_sessions: usize,

        /// Output format
        #[arg(long, value_enum, default_value = "json")]
        format: OutputFormat,

        /// Session source (auto/claude/codex/opencode/all)
        #[arg(long, value_enum, default_value = "auto")]
        source: DataSource,

        /// Custom Claude data directory (default: ~/.claude)
        #[arg(long)]
        claude_dir: Option<PathBuf>,

        /// Custom Codex data directory (default: ~/.codex)
        #[arg(long)]
        codex_dir: Option<PathBuf>,

        /// Custom opencode data directory (default: ~/.local/share/opencode)
        #[arg(long)]
        opencode_dir: Option<PathBuf>,
    },

    /// Fold structured `## Unclear points` from past reports into a
    /// cumulative `reports/ledger.yaml`, then emit a convergence /
    /// divergence signal.
    ///
    /// Run this after generating one or more daily / reflection / insight
    /// reports — each new report becomes one iteration in the streak.
    ///
    /// Pass `--tui` to open an interactive read-only dashboard instead of
    /// printing the summary (requires a build with the `tui` feature).
    Ledger {
        /// Reports directory (default: ./reports)
        #[arg(long, default_value = "reports")]
        reports_dir: PathBuf,

        /// Output ledger path (default: <reports_dir>/ledger.yaml)
        #[arg(long)]
        out: Option<PathBuf>,

        /// Open an interactive read-only dashboard (requires the `tui` feature)
        #[arg(long)]
        tui: bool,

        /// Export recurring General Fix Rules as agent-config candidates
        #[arg(long)]
        export: bool,
    },

    /// Manage Claude Code and Codex skills
    Skill {
        #[command(subcommand)]
        action: SkillAction,
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
            include_prompt_noise,
            include_self,
            max_sessions,
            format,
            source,
            claude_dir,
            codex_dir,
            opencode_dir,
        } => {
            let home_dir = dirs_home();
            let claude_dir = claude_dir.unwrap_or_else(|| home_dir.join(".claude"));
            let codex_dir = codex_dir.unwrap_or_else(|| home_dir.join(".codex"));
            let opencode_dir =
                opencode_dir.unwrap_or_else(|| home_dir.join(".local/share/opencode"));

            // Priority: --period > --from/--to > --days
            let filter = if let Some(ref period) = period {
                DateFilter::from_period(period)
            } else if from.is_some() || to.is_some() {
                DateFilter::from_range(from.as_deref(), to.as_deref())?
            } else {
                DateFilter::from_days(days)
            };

            let selected_sources = resolve_sources(&source, &claude_dir, &codex_dir, &opencode_dir);
            let (mut sessions, total_files) = collect_from_sources(
                &selected_sources,
                &claude_dir,
                &codex_dir,
                &opencode_dir,
                &filter,
            )?;

            if !include_self {
                retain_non_current_sessions(&mut sessions, &current_host_session_ids());
            }
            sessions = merge_sessions_by_id(sessions);

            if !include_prompt_noise {
                retain_meaningful_prompts(&mut sessions);
            }

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
                sort_sessions_by_recency(&mut sessions);
                sessions.truncate(max_sessions);
            }

            let output = build_output(
                sessions,
                &label,
                {
                    let (from, to) = filter.local_date_bounds();
                    OutputPeriod {
                        from,
                        to,
                        timezone: local_timezone_name(),
                    }
                },
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
        Commands::Ledger {
            reports_dir,
            out,
            tui,
            export,
        } => {
            run_ledger(&reports_dir, out.as_deref(), tui, export)?;
        }
        Commands::Skill {
            action: SkillAction::Install { target, force },
        } => {
            let home_dir = resolve_home_dir()?;
            let cwd = std::env::current_dir()?;
            let report = install_skill(&home_dir, &cwd, target, force)?;
            report.print();
        }
    }

    Ok(())
}

fn run_ledger(reports_dir: &Path, out: Option<&Path>, tui: bool, export: bool) -> Result<()> {
    if !reports_dir.is_dir() {
        anyhow::bail!(
            "reports dir not found: {} (run from a nippo project root, or pass --reports-dir)",
            reports_dir.display()
        );
    }
    let default_out = reports_dir.join("ledger.yaml");
    let out_path = out.unwrap_or(&default_out);
    let outcome = ledger::rebuild_from_scratch(reports_dir, out_path)?;
    if export {
        let export_path = reports_dir.join("ledger-export.md");
        ledger::write_export(&export_path, &outcome.ledger)?;
        println!("export: {}", export_path.display());
    }
    if outcome.log.is_empty() {
        println!("ledger: {}", out_path.display());
        println!(
            "(no reports with `## Unclear points` found under {})",
            reports_dir.display()
        );
        return Ok(());
    }
    if tui {
        #[cfg(feature = "tui")]
        {
            crate::ledger_tui::run(&outcome)?;
            return Ok(());
        }
        #[cfg(not(feature = "tui"))]
        {
            anyhow::bail!(
                "--tui requires a build with the `tui` feature: \
                 cargo install --path crates/collector --features tui \
                 (or cargo run -p nippo --features tui -- ledger --tui)"
            );
        }
    }
    println!("ledger: {}", out_path.display());
    for line in &outcome.log {
        println!("  {line}");
    }
    println!();
    match outcome.signal {
        ledger::Signal::Converged => {
            println!(
                "[CONVERGED] {} consecutive report(s) with zero new unclear rules — \
                 this class of struggle has stopped surfacing.",
                ledger::CONVERGE_STREAK
            );
        }
        ledger::Signal::Diverged => {
            println!(
                "[DIVERGENCE-SIGNAL] {} consecutive report(s) with non-decreasing \
                 new-rule count — patching individual symptoms is not working. \
                 Consider a structural change to your workflow / environment / habit, \
                 not another tactical fix.",
                ledger::DIVERGE_STREAK
            );
        }
        ledger::Signal::Continue => {
            println!(
                "[CONTINUE] {} report(s) folded, {} cumulative rule(s) known. \
                 Run again after your next /nippo daily report.",
                outcome.ledger.reports.len(),
                outcome.ledger.known_rules.len(),
            );
        }
    }
    Ok(())
}

fn resolve_sources(
    source: &DataSource,
    claude_dir: &std::path::Path,
    codex_dir: &std::path::Path,
    opencode_dir: &std::path::Path,
) -> Vec<DataSource> {
    match source {
        DataSource::Auto => vec![detect_auto_source(claude_dir, codex_dir, opencode_dir)],
        DataSource::Claude => vec![DataSource::Claude],
        DataSource::Codex => vec![DataSource::Codex],
        DataSource::Opencode => vec![DataSource::Opencode],
        DataSource::All => {
            let mut sources = Vec::new();
            if claude_available(claude_dir) {
                sources.push(DataSource::Claude);
            }
            if codex_available(codex_dir) {
                sources.push(DataSource::Codex);
            }
            if opencode_available(opencode_dir) {
                sources.push(DataSource::Opencode);
            }
            if sources.is_empty() {
                sources.push(detect_auto_source(claude_dir, codex_dir, opencode_dir));
            }
            sources
        }
    }
}

fn detect_auto_source(
    claude_dir: &std::path::Path,
    codex_dir: &std::path::Path,
    opencode_dir: &std::path::Path,
) -> DataSource {
    if std::env::var_os("CODEX_THREAD_ID").is_some() && codex_available(codex_dir) {
        return DataSource::Codex;
    }
    if claude_available(claude_dir) {
        return DataSource::Claude;
    }
    if codex_available(codex_dir) {
        return DataSource::Codex;
    }
    if opencode_available(opencode_dir) {
        return DataSource::Opencode;
    }
    DataSource::Claude
}

fn collect_from_sources(
    sources: &[DataSource],
    claude_dir: &std::path::Path,
    codex_dir: &std::path::Path,
    opencode_dir: &std::path::Path,
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
            DataSource::Opencode => {
                total_files += discover_opencode_history_files(opencode_dir)?.len();
                sessions.extend(collect_opencode_sessions(opencode_dir, filter)?);
            }
            DataSource::Auto | DataSource::All => unreachable!("source must be resolved first"),
        }
    }

    Ok((sessions, total_files))
}

fn retain_non_current_sessions(sessions: &mut Vec<RawSession>, current_ids: &[String]) {
    sessions.retain(|session| {
        !current_ids
            .iter()
            .any(|current_id| current_id == &session.session_id)
    });
}

fn current_host_session_ids() -> Vec<String> {
    ["CLAUDE_CODE_SESSION_ID", "CODEX_THREAD_ID"]
        .into_iter()
        .filter_map(|name| std::env::var(name).ok())
        .filter(|session_id| !session_id.is_empty())
        .collect()
}

fn claude_available(claude_dir: &std::path::Path) -> bool {
    claude_dir.join("projects").exists()
}

fn codex_available(codex_dir: &std::path::Path) -> bool {
    codex_dir.join("history.jsonl").exists()
}

fn opencode_available(opencode_dir: &std::path::Path) -> bool {
    opencode_dir.join("opencode.db").exists() || opencode_dir.join("opencode-dev.db").exists()
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
        DataSource::Opencode => "opencode",
        DataSource::All => "all",
    }
}

/// Resolve the home used by collection defaults. The `/` fallback preserves
/// the existing permissive collection behavior when neither variable is usable.
fn dirs_home() -> PathBuf {
    home_dir_from_values(std::env::var_os("HOME"), std::env::var_os("USERPROFILE"))
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// Resolve the required skill-install destination and reject missing or empty
/// home environment variables instead of writing into an unintended directory.
fn resolve_home_dir() -> Result<PathBuf> {
    home_dir_from_values(std::env::var_os("HOME"), std::env::var_os("USERPROFILE")).ok_or_else(
        || {
            anyhow::anyhow!(
                "HOME and USERPROFILE are unset or empty; cannot determine skill install directory"
            )
        },
    )
}

fn home_dir_from_values(home: Option<OsString>, user_profile: Option<OsString>) -> Option<PathBuf> {
    home.into_iter()
        .chain(user_profile)
        .map(PathBuf::from)
        .find(|path| !path.as_os_str().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw_session(session_id: &str) -> RawSession {
        RawSession {
            session_id: session_id.to_string(),
            project: "nippo".to_string(),
            project_path: "/tmp/nippo".to_string(),
            git_branch: Some("main".to_string()),
            user_entries: Vec::new(),
            assistant_entries: Vec::new(),
        }
    }

    #[test]
    fn excludes_only_the_current_host_sessions() {
        let mut sessions = vec![
            raw_session("claude-current"),
            raw_session("codex-current"),
            raw_session("past-work"),
        ];
        let current_ids = ["claude-current".to_string(), "codex-current".to_string()];

        retain_non_current_sessions(&mut sessions, &current_ids);

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, "past-work");
    }

    #[test]
    fn home_dir_uses_home_then_user_profile() {
        let home = OsString::from("/home/nippo");
        let user_profile = OsString::from(r"C:\Users\yuyas");

        assert_eq!(
            home_dir_from_values(Some(home.clone()), None),
            Some(PathBuf::from("/home/nippo"))
        );
        assert_eq!(
            home_dir_from_values(Some(home), Some(user_profile.clone())),
            Some(PathBuf::from("/home/nippo"))
        );
        assert_eq!(
            home_dir_from_values(None, Some(user_profile.clone())),
            Some(PathBuf::from(r"C:\Users\yuyas"))
        );
        assert_eq!(
            home_dir_from_values(Some(OsString::new()), Some(user_profile)),
            Some(PathBuf::from(r"C:\Users\yuyas"))
        );
        assert_eq!(
            home_dir_from_values(Some(OsString::new()), Some(OsString::new())),
            None
        );
        assert_eq!(home_dir_from_values(None, None), None);
    }

    #[test]
    fn prompt_noise_is_excluded_by_default_and_can_be_included() {
        let default_cli = Cli::try_parse_from(["nippo", "collect"]).expect("parse defaults");
        let Commands::Collect {
            include_prompt_noise,
            ..
        } = default_cli.command
        else {
            panic!("expected collect command");
        };
        assert!(!include_prompt_noise);

        let cli = Cli::try_parse_from([
            "nippo",
            "collect",
            "--include-prompt-noise",
            "--include-self",
        ])
        .expect("parse collect flags");

        let Commands::Collect {
            include_prompt_noise,
            include_self,
            ..
        } = cli.command
        else {
            panic!("expected collect command");
        };
        assert!(include_prompt_noise);
        assert!(include_self);
    }
}
