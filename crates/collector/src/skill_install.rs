use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

const CLAUDE_SKILL: &str = include_str!("../assets/skill-claude.md");
const CODEX_SKILL: &str = include_str!("../assets/skill-codex.md");

const SHARED_ASSETS: &[EmbeddedAsset] = &[
    EmbeddedAsset {
        relative_path: "docs/templates/nippo-template.md",
        contents: include_str!("../assets/docs/templates/nippo-template.md"),
    },
    EmbeddedAsset {
        relative_path: "docs/templates/reflection-template.md",
        contents: include_str!("../assets/docs/templates/reflection-template.md"),
    },
    EmbeddedAsset {
        relative_path: "docs/templates/guide-template.md",
        contents: include_str!("../assets/docs/templates/guide-template.md"),
    },
    EmbeddedAsset {
        relative_path: "docs/templates/report-template.md",
        contents: include_str!("../assets/docs/templates/report-template.md"),
    },
    EmbeddedAsset {
        relative_path: "docs/templates/review-template.md",
        contents: include_str!("../assets/docs/templates/review-template.md"),
    },
    EmbeddedAsset {
        relative_path: "docs/templates/insight-template.md",
        contents: include_str!("../assets/docs/templates/insight-template.md"),
    },
    EmbeddedAsset {
        relative_path: "docs/templates/trend-template.md",
        contents: include_str!("../assets/docs/templates/trend-template.md"),
    },
    EmbeddedAsset {
        relative_path: "docs/templates/plan-template.md",
        contents: include_str!("../assets/docs/templates/plan-template.md"),
    },
    EmbeddedAsset {
        relative_path: "docs/reflection-theory.md",
        contents: include_str!("../assets/docs/reflection-theory.md"),
    },
    EmbeddedAsset {
        relative_path: "docs/data-sources.md",
        contents: include_str!("../assets/docs/data-sources.md"),
    },
];

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum SkillTarget {
    Claude,
    Codex,
    Copilot,
    All,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SkillKind {
    Claude,
    Codex,
    Copilot,
}

#[derive(Clone, Copy)]
struct EmbeddedAsset {
    relative_path: &'static str,
    contents: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InstallMode {
    Symlink,
    Embedded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExistingState {
    Vacant,
    CorrectSymlink,
    Replace,
}

#[derive(Debug)]
struct InstallPlan {
    kind: SkillKind,
    destination: PathBuf,
    source: Option<PathBuf>,
    existing: ExistingState,
}

#[derive(Debug)]
pub(crate) struct InstallReport {
    mode: InstallMode,
    installed: Vec<(SkillKind, PathBuf, ExistingState)>,
}

impl SkillTarget {
    fn kinds(self) -> &'static [SkillKind] {
        match self {
            Self::Claude => &[SkillKind::Claude],
            Self::Codex => &[SkillKind::Codex],
            Self::Copilot => &[SkillKind::Copilot],
            Self::All => &[SkillKind::Claude, SkillKind::Codex, SkillKind::Copilot],
        }
    }
}

impl SkillKind {
    fn label(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Codex => "Codex",
            Self::Copilot => "GitHub Copilot",
        }
    }

    fn relative_install_path(self) -> &'static str {
        match self {
            Self::Claude => ".claude/skills/nippo",
            Self::Codex => ".agents/skills/nippo",
            Self::Copilot => ".copilot/skills/nippo",
        }
    }

    fn relative_source_path(self) -> &'static str {
        match self {
            Self::Claude => ".claude/skills/nippo",
            Self::Codex | Self::Copilot => ".agents/skills/nippo",
        }
    }

    fn skill_contents(self) -> &'static str {
        match self {
            Self::Claude => CLAUDE_SKILL,
            Self::Codex | Self::Copilot => CODEX_SKILL,
        }
    }

    fn next_step(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code では `/nippo`",
            Self::Codex => "Codex では `$nippo`",
            Self::Copilot => "GitHub Copilot では `/nippo`",
        }
    }
}

impl InstallReport {
    pub(crate) fn print(&self) {
        let mode = match self.mode {
            InstallMode::Symlink => "シンボリックリンク",
            InstallMode::Embedded => "埋め込みファイルの書き出し",
        };
        println!("インストール方式: {mode}");
        for (kind, destination, existing) in &self.installed {
            let status = if *existing == ExistingState::CorrectSymlink {
                "インストール済み"
            } else {
                "インストールしました"
            };
            println!("{status}: {} -> {}", kind.label(), destination.display());
        }
        println!("次の一歩:");
        for (kind, _, _) in &self.installed {
            println!("  - {} を実行", kind.next_step());
        }
    }
}

pub(crate) fn install(
    home_dir: &Path,
    cwd: &Path,
    target: SkillTarget,
    force: bool,
) -> Result<InstallReport> {
    let repo_root = find_repo_root(cwd)?;
    let mode = if repo_root.is_some() {
        InstallMode::Symlink
    } else {
        InstallMode::Embedded
    };

    let mut plans = Vec::new();
    for &kind in target.kinds() {
        let destination = home_dir.join(kind.relative_install_path());
        let source = repo_root
            .as_ref()
            .map(|root| root.join(kind.relative_source_path()));
        match source.as_deref() {
            Some(source) if !source.is_dir() => {
                bail!("skill source not found: {}", source.display());
            }
            _ => {}
        }
        let existing = inspect_existing(&destination, source.as_deref())?;
        if existing == ExistingState::Replace && !force {
            bail!(
                "{} already exists and is not the expected nippo symlink: {}. Re-run with --force to replace it",
                kind.label(),
                destination.display()
            );
        }
        plans.push(InstallPlan {
            kind,
            destination,
            source,
            existing,
        });
    }

    let mut installed = Vec::new();
    for plan in plans {
        if plan.existing == ExistingState::CorrectSymlink {
            installed.push((plan.kind, plan.destination, plan.existing));
            continue;
        }
        if plan.existing == ExistingState::Replace {
            remove_existing(&plan.destination)?;
        }

        match mode {
            InstallMode::Symlink => {
                let source = plan
                    .source
                    .as_deref()
                    .context("repository install source was not resolved")?;
                install_symlink(source, &plan.destination)?;
            }
            InstallMode::Embedded => install_embedded(plan.kind, &plan.destination)?,
        }
        installed.push((plan.kind, plan.destination, plan.existing));
    }

    Ok(InstallReport { mode, installed })
}

fn find_repo_root(cwd: &Path) -> Result<Option<PathBuf>> {
    for candidate in cwd.ancestors() {
        let manifest_path = candidate.join("crates/collector/Cargo.toml");
        let claude_skill = candidate.join(".claude/skills/nippo");
        if !manifest_path.is_file() || !claude_skill.is_dir() {
            continue;
        }
        let manifest = fs::read_to_string(&manifest_path)
            .with_context(|| format!("failed to read {}", manifest_path.display()))?;
        if manifest.contains("name = \"nippo\"") {
            return Ok(Some(candidate.to_path_buf()));
        }
    }
    Ok(None)
}

fn inspect_existing(destination: &Path, expected_source: Option<&Path>) -> Result<ExistingState> {
    let metadata = match fs::symlink_metadata(destination) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(ExistingState::Vacant),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to inspect {}", destination.display()));
        }
    };

    let is_correct_symlink = if metadata.file_type().is_symlink() {
        match expected_source {
            Some(expected_source) => symlink_points_to(destination, expected_source)?,
            None => false,
        }
    } else {
        false
    };
    if is_correct_symlink {
        return Ok(ExistingState::CorrectSymlink);
    }
    Ok(ExistingState::Replace)
}

fn symlink_points_to(destination: &Path, expected_source: &Path) -> Result<bool> {
    let expected = fs::canonicalize(expected_source)
        .with_context(|| format!("skill source not found: {}", expected_source.display()))?;
    match fs::canonicalize(destination) {
        Ok(actual) => Ok(actual == expected),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error)
            .with_context(|| format!("failed to resolve symlink {}", destination.display())),
    }
}

fn remove_existing(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect {}", path.display()))?;
    if metadata.file_type().is_symlink() || metadata.is_file() {
        fs::remove_file(path).with_context(|| format!("failed to remove {}", path.display()))?;
    } else {
        fs::remove_dir_all(path).with_context(|| format!("failed to remove {}", path.display()))?;
    }
    Ok(())
}

fn install_embedded(kind: SkillKind, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)
        .with_context(|| format!("failed to create {}", destination.display()))?;
    fs::write(destination.join("SKILL.md"), kind.skill_contents())
        .with_context(|| format!("failed to write {}/SKILL.md", destination.display()))?;

    for asset in SHARED_ASSETS {
        let path = destination.join(asset.relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        fs::write(&path, asset.contents)
            .with_context(|| format!("failed to write {}", path.display()))?;
    }
    Ok(())
}

fn install_symlink(source: &Path, destination: &Path) -> Result<()> {
    if !source.is_dir() {
        bail!("skill source not found: {}", source.display());
    }
    let parent = destination
        .parent()
        .context("skill install destination has no parent")?;
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;
    create_dir_symlink(source, destination).with_context(|| {
        format!(
            "failed to link {} -> {}",
            destination.display(),
            source.display()
        )
    })
}

#[cfg(unix)]
fn create_dir_symlink(source: &Path, destination: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(source, destination)
}

#[cfg(windows)]
fn create_dir_symlink(source: &Path, destination: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_dir(source, destination)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_repository_fixture(repo: &Path) -> Result<()> {
        fs::write(
            repo.join("Cargo.toml"),
            "[workspace]\nresolver = \"2\"\nmembers = [\n  \"crates/collector\",\n  \"crates/other\",\n]\n",
        )?;
        let collector_dir = repo.join("crates/collector");
        fs::create_dir_all(&collector_dir)?;
        fs::write(
            collector_dir.join("Cargo.toml"),
            "[package]\nversion = \"0.1.4\"\nname = \"nippo\"\n",
        )?;
        for kind in [SkillKind::Claude, SkillKind::Codex] {
            let source = repo.join(kind.relative_source_path());
            fs::create_dir_all(&source)?;
            fs::write(source.join("SKILL.md"), kind.skill_contents())?;
        }
        Ok(())
    }

    #[test]
    fn embedded_assets_are_not_empty() {
        assert!(!CLAUDE_SKILL.trim().is_empty());
        assert!(!CODEX_SKILL.trim().is_empty());
        for asset in SHARED_ASSETS {
            assert!(
                !asset.contents.trim().is_empty(),
                "embedded asset is empty: {}",
                asset.relative_path
            );
        }
    }

    #[test]
    fn installs_all_embedded_files() -> Result<()> {
        let home = TempDir::new().context("create temp home")?;
        let cwd = TempDir::new().context("create temp cwd")?;

        install(home.path(), cwd.path(), SkillTarget::All, false)?;

        for kind in [SkillKind::Claude, SkillKind::Codex, SkillKind::Copilot] {
            let destination = home.path().join(kind.relative_install_path());
            assert_eq!(
                fs::read_to_string(destination.join("SKILL.md"))?,
                kind.skill_contents()
            );
            for asset in SHARED_ASSETS {
                assert_eq!(
                    fs::read_to_string(destination.join(asset.relative_path))?,
                    asset.contents
                );
            }
        }
        Ok(())
    }

    #[test]
    fn existing_install_requires_force_and_force_replaces_it() -> Result<()> {
        let home = TempDir::new().context("create temp home")?;
        let cwd = TempDir::new().context("create temp cwd")?;
        let destination = home.path().join(SkillKind::Claude.relative_install_path());
        fs::create_dir_all(&destination)?;
        fs::write(destination.join("old.txt"), "old install")?;

        let error = install(home.path(), cwd.path(), SkillTarget::Claude, false)
            .expect_err("existing install should require --force");
        assert!(error.to_string().contains("--force"));

        install(home.path(), cwd.path(), SkillTarget::Claude, true)?;
        assert!(!destination.join("old.txt").exists());
        assert_eq!(
            fs::read_to_string(destination.join("SKILL.md"))?,
            CLAUDE_SKILL
        );
        Ok(())
    }

    #[test]
    fn target_limits_install_to_selected_host() -> Result<()> {
        let home = TempDir::new().context("create temp home")?;
        let cwd = TempDir::new().context("create temp cwd")?;

        install(home.path(), cwd.path(), SkillTarget::Codex, false)?;

        assert!(
            !home
                .path()
                .join(SkillKind::Claude.relative_install_path())
                .exists()
        );
        assert!(
            home.path()
                .join(SkillKind::Codex.relative_install_path())
                .join("SKILL.md")
                .is_file()
        );
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn installs_repository_symlinks_and_accepts_correct_existing_links() -> Result<()> {
        let home = TempDir::new().context("create temp home")?;
        let repo = TempDir::new().context("create temp repo")?;
        create_repository_fixture(repo.path())?;
        let nested_cwd = repo.path().join("crates/collector");

        install(home.path(), &nested_cwd, SkillTarget::All, false)?;
        install(home.path(), &nested_cwd, SkillTarget::All, false)?;

        for kind in [SkillKind::Claude, SkillKind::Codex, SkillKind::Copilot] {
            let destination = home.path().join(kind.relative_install_path());
            assert!(fs::symlink_metadata(&destination)?.file_type().is_symlink());
            assert!(symlink_points_to(
                &destination,
                &repo.path().join(kind.relative_source_path())
            )?);
        }
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn unexpected_symlink_requires_force_and_only_replaces_the_link() -> Result<()> {
        let home = TempDir::new().context("create temp home")?;
        let repo = TempDir::new().context("create temp repo")?;
        let unrelated = TempDir::new().context("create unrelated target")?;
        create_repository_fixture(repo.path())?;

        let destination = home.path().join(SkillKind::Claude.relative_install_path());
        let parent = destination.parent().context("destination parent")?;
        fs::create_dir_all(parent)?;
        fs::write(unrelated.path().join("keep.txt"), "preserve me")?;
        std::os::unix::fs::symlink(unrelated.path(), &destination)?;

        let error = install(
            home.path(),
            &repo.path().join("crates/collector"),
            SkillTarget::Claude,
            false,
        )
        .expect_err("unexpected symlink should require --force");
        assert!(error.to_string().contains("--force"));
        assert!(symlink_points_to(&destination, unrelated.path())?);

        install(
            home.path(),
            &repo.path().join("crates/collector"),
            SkillTarget::Claude,
            true,
        )?;

        assert!(fs::symlink_metadata(&destination)?.file_type().is_symlink());
        assert!(symlink_points_to(
            &destination,
            &repo.path().join(SkillKind::Claude.relative_install_path())
        )?);
        assert_eq!(
            fs::read_to_string(unrelated.path().join("keep.txt"))?,
            "preserve me"
        );
        Ok(())
    }
}
