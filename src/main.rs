use anyhow::{Context, Result, bail};
use clap::{CommandFactory, Parser, Subcommand, ValueEnum, ValueHint};
use clap_complete::engine::{ArgValueCompleter, CompletionCandidate};
use engine::{Config, LinkMethod};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;
use uncle_skills_manager::{config, engine};

#[derive(Parser)]
#[command(
    name = "uncle-skills",
    version,
    about = "Keep your skills linked across coding agents"
)]
struct Cli {
    /// Machine-local configuration file (never put credentials here)
    #[arg(long, global = true, env = "UNCLE_SKILLS_CONFIG", value_hint = ValueHint::FilePath)]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Action,
}

#[derive(Clone, Copy, ValueEnum)]
enum Agent {
    Codex,
    ClaudeCode,
    Pi,
    GeminiCli,
}
impl Agent {
    fn id(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::ClaudeCode => "claude-code",
            Self::Pi => "pi",
            Self::GeminiCli => "gemini-cli",
        }
    }
}
#[derive(Clone, Copy, ValueEnum)]
enum Method {
    Symlink,
    Copy,
}
impl From<Method> for LinkMethod {
    fn from(value: Method) -> Self {
        match value {
            Method::Symlink => Self::Symlink,
            Method::Copy => Self::Copy,
        }
    }
}

#[derive(Subcommand)]
enum Action {
    /// Register an existing local repository and agent locations; does not deploy skills
    Init {
        #[arg(long, value_hint = ValueHint::DirPath)]
        repo: PathBuf,
        /// Repeat to select agents; otherwise use supported agents found on PATH
        #[arg(long, value_enum)]
        agent: Vec<Agent>,
        /// Claude's effective config directory when set inside a custom launcher
        #[arg(long, value_hint = ValueHint::DirPath)]
        claude_config_dir: Option<PathBuf>,
        /// Override a skills directory, e.g. --target claude-code=/path/to/skills
        #[arg(long, value_name = "AGENT=PATH")]
        target: Vec<String>,
        /// Copies detect edits before replacement; symlinks read the source directly
        #[arg(long, value_enum, default_value = "symlink")]
        method: Method,
    },
    /// Show supported agents, PATH detection, and resolved default locations
    Agents,
    /// List valid skills in the configured source repository
    List,
    /// Print a source skill without executing it
    Show {
        #[arg(add = ArgValueCompleter::new(skill_candidates))]
        skill: String,
    },
    /// Reconcile selected skills with agent locations
    Sync {
        /// Preview without changing the filesystem
        #[arg(long)]
        dry_run: bool,
        /// Explicitly take ownership of identical existing installations
        #[arg(long)]
        adopt_existing: bool,
    },
    /// Show pending deployment changes without changing files
    Status,
    /// Validate source skills and deployment paths without changing files
    Doctor,
    /// Fast-forward the clean repository from its upstream, then reconcile skills
    Pull,
    /// Print shell completion setup (zsh and bash query current skill names)
    Completions {
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

fn skill_candidates(current: &OsStr) -> Vec<CompletionCandidate> {
    let mut args = std::env::args_os().peekable();
    let mut path = std::env::var_os("UNCLE_SKILLS_CONFIG").map(PathBuf::from);
    while let Some(arg) = args.next() {
        if arg == "--config" {
            path = args.next().map(PathBuf::from);
        } else if let Some(s) = arg.to_str().and_then(|s| s.strip_prefix("--config=")) {
            path = Some(PathBuf::from(s));
        }
    }
    let Ok(path) = path.map(Ok).unwrap_or_else(config::default_config_path) else {
        return vec![];
    };
    let Ok(cfg) = config::load(&path) else {
        return vec![];
    };
    let Ok(names) = engine::discover_skills(&cfg.repo) else {
        return vec![];
    };
    let prefix = current.to_string_lossy();
    names
        .into_iter()
        .filter(|s| s.starts_with(prefix.as_ref()))
        .map(CompletionCandidate::new)
        .collect()
}

fn main() {
    clap_complete::CompleteEnv::with_factory(Cli::command).complete();
    if let Err(error) = run(Cli::parse()) {
        eprintln!("Error: {error:#}");
        std::process::exit(1);
    }
}

fn print_changes(changes: Vec<String>) {
    if changes.is_empty() {
        println!("Up to date.");
    } else {
        for change in changes {
            println!("{change}");
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    // Completion setup and agent inspection do not require an existing configuration.
    if let Action::Completions { shell } = cli.command {
        match shell {
            clap_complete::Shell::Zsh | clap_complete::Shell::Bash => {
                let exe = std::env::current_exe()?;
                let output = Command::new(exe)
                    .env("COMPLETE", shell.to_string())
                    .output()?;
                if !output.status.success() {
                    bail!("Could not generate completion setup");
                }
                print!("{}", String::from_utf8(output.stdout)?);
            }
            _ => clap_complete::generate(
                shell,
                &mut Cli::command(),
                "uncle-skills",
                &mut std::io::stdout(),
            ),
        }
        return Ok(());
    }
    if matches!(cli.command, Action::Agents) {
        for (id, executable) in config::AGENTS {
            let located = which::which(executable).ok();
            let target = config::target(id, LinkMethod::Symlink, None)?;
            println!(
                "{id}: {} | {}",
                located
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "not on PATH".into()),
                target.skills_dir.display()
            );
        }
        println!(
            "Custom launchers can hide environment overrides. Register --target or --claude-config-dir explicitly."
        );
        return Ok(());
    }
    let config_path = config::absolute(
        &cli.config
            .map(Ok)
            .unwrap_or_else(config::default_config_path)?,
    )?;
    if let Action::Init {
        repo,
        agent,
        claude_config_dir,
        target,
        method,
    } = cli.command
    {
        let repo = config::absolute(&repo)?
            .canonicalize()
            .context("Repository directory does not exist")?;
        let names = engine::discover_skills(&repo)?;
        let mut ids: Vec<String> = agent.into_iter().map(|a| a.id().to_owned()).collect();
        if ids.is_empty() {
            ids = config::AGENTS
                .iter()
                .filter(|(_, cmd)| which::which(cmd).is_ok())
                .map(|(id, _)| (*id).to_owned())
                .collect();
        }
        let mut overrides = std::collections::BTreeMap::new();
        for entry in target {
            let (id, path) = entry.split_once('=').context("Use --target AGENT=PATH")?;
            if path.is_empty() {
                bail!("Target path cannot be empty");
            }
            if !config::AGENTS.iter().any(|(known, _)| *known == id) {
                bail!("Unknown agent {id}");
            }
            if overrides
                .insert(id.to_owned(), config::absolute(Path::new(path))?)
                .is_some()
            {
                bail!("Duplicate target for {id}");
            }
            if !ids.iter().any(|known| known == id) {
                ids.push(id.to_owned());
            }
        }
        ids.sort();
        ids.dedup();
        if ids.is_empty() {
            bail!("No supported agents found; specify --agent or --target");
        }
        let mut agents = vec![];
        for id in ids {
            let mut entry = config::target(&id, method.into(), claude_config_dir.as_deref())?;
            if let Some(path) = overrides.remove(&id) {
                entry.skills_dir = path;
            }
            agents.push(entry);
        }
        let config = Config {
            version: 1,
            repo,
            agents,
        };
        // A dry-run validates destinations before configuration is saved.
        engine::sync(&config, &config::state_path(&config_path), true, false)?;
        config::save_new(&config_path, &config)?;
        println!(
            "Registered {} skills in {}",
            names.len(),
            config_path.display()
        );
        for agent in &config.agents {
            println!("{} -> {}", agent.id, agent.skills_dir.display());
        }
        println!("Next: uncle-skills sync --dry-run, then uncle-skills sync");
        return Ok(());
    }
    let cfg = config::load(&config_path)?;
    let state = config::state_path(&config_path);
    match cli.command {
        Action::List => {
            for name in engine::discover_skills(&cfg.repo)? {
                println!("{name}");
            }
        }
        Action::Show { skill } => {
            if !engine::discover_skills(&cfg.repo)?.contains(&skill) {
                bail!("Unknown skill {skill}; use uncle-skills list");
            }
            print!(
                "{}",
                std::fs::read_to_string(cfg.repo.join("skills").join(skill).join("SKILL.md"))?
            );
        }
        Action::Sync {
            dry_run,
            adopt_existing,
        } => {
            print_changes(engine::sync(&cfg, &state, dry_run, adopt_existing)?);
            if !dry_run {
                println!(
                    "Refresh the agent's skill list; reload or restart if changes do not appear."
                );
            }
        }
        Action::Status => print_changes(engine::sync(&cfg, &state, true, false)?),
        Action::Doctor => {
            let names = engine::discover_skills(&cfg.repo)?;
            let changes = engine::sync(&cfg, &state, true, false)?;
            println!("Configuration: {}", config_path.display());
            println!(
                "{} valid skills; {} configured agents.",
                names.len(),
                cfg.agents.len()
            );
            print_changes(changes);
            println!(
                "Filesystem checks passed. Native agent discovery and skill behavior must be checked separately."
            );
        }
        Action::Pull => {
            // Refuse deployment conflicts before touching the checkout.
            engine::sync(&cfg, &state, true, false)?;
            git_pull(&cfg.repo)?;
            print_changes(engine::sync(&cfg, &state, false, false)?);
            println!("Pulled and reconciled. Refresh the agent's skill list if needed.");
        }
        Action::Init { .. } | Action::Agents | Action::Completions { .. } => unreachable!(),
    }
    Ok(())
}

fn git_pull(repo: &Path) -> Result<()> {
    let top = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("Git is required for pull")?;
    if !top.status.success() {
        bail!("Configured source is not a Git repository");
    }
    let top = PathBuf::from(String::from_utf8(top.stdout)?.trim());
    if top.canonicalize()? != repo.canonicalize()? {
        bail!(
            "Configured source must be the Git repository root; refusing to pull an enclosing repository"
        );
    }
    let status = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["status", "--porcelain"])
        .output()
        .context("Git is required for pull")?;
    if !status.status.success() {
        bail!("Cannot inspect repository status");
    }
    if !status.stdout.is_empty() {
        bail!("Repository has local changes; commit or resolve them before pulling");
    }
    let branch = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["symbolic-ref", "--quiet", "HEAD"])
        .output()?;
    if !branch.status.success() {
        bail!("Repository has a detached HEAD; select a branch before pulling");
    }
    let result = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["pull", "--ff-only"])
        .status()?;
    if !result.success() {
        bail!(
            "Git pull stopped; resolve authentication, upstream, or divergent history before retrying"
        );
    }
    Ok(())
}
