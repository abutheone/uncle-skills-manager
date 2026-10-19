use anyhow::{Context, Result, bail};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::engine::{AgentTarget, Config, LinkMethod};

pub const AGENTS: &[(&str, &str)] = &[
    ("codex", "codex"),
    ("claude-code", "claude"),
    ("pi", "pi"),
    ("gemini-cli", "gemini"),
];

pub fn default_config_path() -> Result<PathBuf> {
    if let Some(path) = env::var_os("UNCLE_SKILLS_CONFIG") {
        return absolute(Path::new(&path));
    }
    let root = dirs::config_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join(".config")))
        .context("Cannot locate a configuration directory; use --config PATH")?;
    Ok(root.join("uncle-skills").join("config.toml"))
}

pub fn absolute(path: &Path) -> Result<PathBuf> {
    let expanded = if path == Path::new("~") {
        dirs::home_dir().context("Cannot expand ~ without a home directory")?
    } else if let Ok(rest) = path.strip_prefix("~/") {
        dirs::home_dir()
            .context("Cannot expand ~ without a home directory")?
            .join(rest)
    } else {
        path.to_path_buf()
    };
    std::path::absolute(expanded).context("Cannot resolve path")
}

pub fn load(path: &Path) -> Result<Config> {
    let text = fs::read_to_string(path).with_context(|| {
        format!(
            "Cannot read {}; run uncle-skills init first",
            path.display()
        )
    })?;
    let config: Config = toml::from_str(&text)
        .with_context(|| format!("Invalid configuration: {}", path.display()))?;
    if config.version != 1 {
        bail!(
            "Unsupported configuration version {}; expected 1",
            config.version
        );
    }
    Ok(config)
}

pub fn state_path(config_path: &Path) -> PathBuf {
    config_path.with_extension("state.json")
}

pub fn save_new(path: &Path, config: &Config) -> Result<()> {
    let parent = path
        .parent()
        .context("Configuration needs a parent directory")?;
    fs::create_dir_all(parent)?;
    let mut staged = tempfile::NamedTempFile::new_in(parent)?;
    use std::io::Write;
    staged.write_all(toml::to_string_pretty(config)?.as_bytes())?;
    staged.as_file().sync_all()?;
    staged.persist_noclobber(path).with_context(|| {
        format!(
            "Cannot create {}; existing configuration is never overwritten",
            path.display()
        )
    })?;
    Ok(())
}

pub fn target(id: &str, method: LinkMethod, explicit_claude: Option<&Path>) -> Result<AgentTarget> {
    let home = dirs::home_dir().context("Cannot locate home directory; use explicit targets")?;
    let skills_dir = match id {
        "codex" | "pi" => home.join(".agents/skills"),
        "claude-code" => {
            let base = explicit_claude
                .map(Path::to_path_buf)
                .or_else(|| {
                    env::var_os("CLAUDE_CONFIG_DIR")
                        .filter(|s| !s.is_empty())
                        .map(PathBuf::from)
                })
                .unwrap_or_else(|| home.join(".claude"));
            absolute(&base)?.join("skills")
        }
        "gemini-cli" => home.join(".gemini/skills"),
        _ => bail!("Unknown agent {id}; supported: codex, claude-code, pi, gemini-cli"),
    };
    Ok(AgentTarget {
        id: id.to_owned(),
        skills_dir,
        method,
        skills: None,
    })
}
