//! Local-only deployment. Skill code is never executed.
use anyhow::{Context, Result, bail};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};
use walkdir::WalkDir;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LinkMethod {
    Symlink,
    Copy,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentTarget {
    pub id: String,
    pub skills_dir: PathBuf,
    pub method: LinkMethod,
    #[serde(default)]
    pub skills: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub version: u32,
    pub repo: PathBuf,
    pub agents: Vec<AgentTarget>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    source: PathBuf,
    destination: PathBuf,
    method: LinkMethod,
    hash: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct State {
    version: u32,
    entries: BTreeMap<PathBuf, Entry>,
}

#[derive(Deserialize)]
struct Metadata {
    name: String,
    description: String,
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && !name.starts_with('-')
        && !name.ends_with('-')
        && !name.contains("--")
        && name
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}

/// Validate immediate skill directories; ordinary repository files are ignored.
pub fn discover_skills(repo: &Path) -> Result<Vec<String>> {
    let directory = repo.join("skills");
    let info = fs::symlink_metadata(&directory)
        .with_context(|| format!("Cannot inspect {}", directory.display()))?;
    if !info.is_dir() || info.file_type().is_symlink() {
        bail!(
            "Skills root must be a real directory: {}",
            directory.display()
        );
    }
    let mut names = Vec::new();
    for item in fs::read_dir(&directory).with_context(|| {
        format!(
            "Cannot read {}; create a skills directory first",
            directory.display()
        )
    })? {
        let item = item?;
        let kind = item.file_type()?;
        if kind.is_symlink() {
            bail!(
                "Source skill cannot be a symlink: {}",
                item.path().display()
            );
        }
        if !kind.is_dir() {
            continue;
        }
        let name = item
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("Skill folder names must be UTF-8"))?;
        if !valid_name(&name) {
            bail!(
                "Invalid skill folder name {name:?}; use lowercase ASCII letters, digits and single hyphens (1–64 characters)"
            );
        }
        let path = item.path().join("SKILL.md");
        let info = fs::symlink_metadata(&path)
            .with_context(|| format!("Cannot inspect {}", path.display()))?;
        if !info.is_file() || info.file_type().is_symlink() || info.len() > 1024 * 1024 {
            bail!(
                "{} must be a regular file no larger than 1 MiB",
                path.display()
            );
        }
        let text =
            fs::read_to_string(&path).with_context(|| format!("Cannot read {}", path.display()))?;
        let normalized = text.replace("\r\n", "\n");
        let rest = normalized
            .strip_prefix("---\n")
            .with_context(|| format!("{} must start with YAML frontmatter", path.display()))?;
        let end = rest
            .find("\n---\n")
            .or_else(|| rest.strip_suffix("\n---").map(str::len))
            .with_context(|| {
                format!(
                    "{} has no closing YAML frontmatter delimiter",
                    path.display()
                )
            })?;
        let body = rest[end..].strip_prefix("\n---\n").unwrap_or("");
        if body.trim().is_empty() {
            bail!(
                "{} must contain nonempty instructions after frontmatter",
                path.display()
            );
        }
        let metadata: Metadata = serde_yaml_ng::from_str(&rest[..end])
            .with_context(|| format!("Invalid metadata in {}", path.display()))?;
        if metadata.name != name {
            bail!("{} name must match folder {name:?}", path.display());
        }
        if metadata.description.trim().is_empty() || metadata.description.chars().count() > 1024 {
            bail!(
                "{} description must contain 1–1024 characters",
                path.display()
            );
        }
        tree_hash(&item.path())?;
        names.push(name);
    }
    names.sort();
    Ok(names)
}

// Includes directory structure, names, file contents and Unix executable mode.
fn tree_hash(root: &Path) -> Result<String> {
    let metadata =
        fs::symlink_metadata(root).with_context(|| format!("Cannot inspect {}", root.display()))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        bail!("Expected a real directory at {}", root.display());
    }
    let mut digest = Sha256::new();
    for item in WalkDir::new(root).follow_links(false).sort_by_file_name() {
        let item = item?;
        let name = item
            .path()
            .strip_prefix(root)?
            .to_str()
            .with_context(|| format!("Non-UTF-8 source path {}", item.path().display()))?
            .replace('\\', "/");
        let kind = item.file_type();
        if kind.is_symlink() || !(kind.is_file() || kind.is_dir()) {
            bail!(
                "Unsupported symlink or special file: {}",
                item.path().display()
            );
        }
        digest.update(if kind.is_dir() { b"d" } else { b"f" });
        digest.update((name.len() as u64).to_le_bytes());
        digest.update(name.as_bytes());
        if kind.is_file() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                digest.update((item.metadata()?.permissions().mode() & 0o111).to_le_bytes());
            }
            let mut file = File::open(item.path())?;
            let expected_length = file.metadata()?.len();
            digest.update(expected_length.to_le_bytes());
            let mut buffer = [0_u8; 64 * 1024];
            let mut total = 0_u64;
            loop {
                let count = file.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                total += count as u64;
                digest.update(&buffer[..count]);
            }
            if total != expected_length {
                bail!(
                    "File changed while hashing: {}; finish edits and retry",
                    item.path().display()
                );
            }
        }
    }
    Ok(digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e).with_context(|| format!("Cannot inspect {}", path.display())),
    }
}
fn absolute_clean(path: &Path) -> Result<()> {
    if !path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        bail!(
            "Use an absolute path without . or .. components: {}",
            path.display()
        );
    }
    Ok(())
}
fn physical_path(path: &Path) -> Result<PathBuf> {
    if exists(path)? {
        return Ok(fs::canonicalize(path)?);
    }
    Ok(physical_path(path.parent().context("Path has no parent")?)?
        .join(path.file_name().context("Path has no filename")?))
}
fn overlaps(a: &Path, b: &Path) -> bool {
    a.starts_with(b) || b.starts_with(a)
}

fn desired(config: &Config, state_path: &Path) -> Result<BTreeMap<PathBuf, Entry>> {
    if config.version != 1 {
        bail!(
            "Unsupported configuration version {}; expected 1",
            config.version
        );
    }
    absolute_clean(&config.repo)?;
    let repo = fs::canonicalize(&config.repo).context("Cannot resolve skills repository")?;
    let names = discover_skills(&repo)?;
    let mut result = BTreeMap::new();
    let mut identifiers = BTreeSet::new();
    let mut directories = BTreeMap::<PathBuf, (LinkMethod, Vec<String>)>::new();
    let physical_state = physical_path(state_path)?;
    for agent in &config.agents {
        if !valid_name(&agent.id) || !identifiers.insert(&agent.id) {
            bail!(
                "Agent IDs must be unique safe lowercase names: {:?}",
                agent.id
            );
        }
        absolute_clean(&agent.skills_dir)?;
        let directory = physical_path(&agent.skills_dir)?;
        if overlaps(&directory, &repo) || overlaps(&directory, &physical_state) {
            bail!(
                "Agent skills directories must be separate from each other, the repository, and ownership state: {}",
                directory.display()
            );
        }
        let mut selected = agent.skills.clone().unwrap_or_else(|| names.clone());
        selected.sort();
        if let Some((method, previous)) = directories.get(&directory) {
            if method != &agent.method || previous != &selected {
                bail!(
                    "Agents sharing {} must use the same method and skill selection",
                    directory.display()
                );
            }
            continue;
        }
        if directories.keys().any(|other| overlaps(&directory, other)) {
            bail!(
                "Agent skills directories cannot be nested: {}",
                directory.display()
            );
        }
        directories.insert(directory.clone(), (agent.method.clone(), selected.clone()));
        let mut unique = BTreeSet::new();
        for name in &selected {
            if !valid_name(name) || !names.contains(name) || !unique.insert(name) {
                bail!(
                    "Unknown, invalid or duplicate selected skill {name:?} for {}",
                    agent.id
                );
            }
            let source = repo.join("skills").join(name);
            let destination = directory.join(name);
            result.insert(
                destination.clone(),
                Entry {
                    hash: tree_hash(&source)?,
                    source,
                    destination,
                    method: agent.method.clone(),
                },
            );
        }
    }
    Ok(result)
}
fn matches_entry(entry: &Entry) -> Result<bool> {
    let metadata = fs::symlink_metadata(&entry.destination)?;
    match entry.method {
        LinkMethod::Symlink => {
            if !metadata.file_type().is_symlink() {
                return Ok(false);
            }
            let target = fs::read_link(&entry.destination)?;
            let target = if target.is_absolute() {
                target
            } else {
                entry
                    .destination
                    .parent()
                    .context("Destination has no parent")?
                    .join(target)
            };
            Ok(physical_path(&target)? == physical_path(&entry.source)?)
        }
        LinkMethod::Copy => Ok(metadata.is_dir()
            && !metadata.file_type().is_symlink()
            && tree_hash(&entry.destination)
                .map(|h| h == entry.hash)
                .unwrap_or(false)),
    }
}
fn same(old: &Entry, next: &Entry) -> bool {
    old.source == next.source
        && old.method == next.method
        && (old.method == LinkMethod::Symlink || old.hash == next.hash)
}
fn remove_owned(entry: &Entry) -> Result<()> {
    if !exists(&entry.destination)? {
        return Ok(());
    }
    if !matches_entry(entry)? {
        bail!(
            "Refusing to change {}: managed installation changed; preserve your edits and resolve manually",
            entry.destination.display()
        );
    }
    match entry.method {
        LinkMethod::Copy => fs::remove_dir_all(&entry.destination)?,
        LinkMethod::Symlink => {
            #[cfg(windows)]
            fs::remove_dir(&entry.destination)?;
            #[cfg(not(windows))]
            fs::remove_file(&entry.destination)?;
        }
    }
    Ok(())
}
fn install(entry: &Entry) -> Result<()> {
    fs::create_dir_all(
        entry
            .destination
            .parent()
            .context("Destination has no parent")?,
    )?;
    match entry.method {
        LinkMethod::Symlink => {
            #[cfg(unix)]
            std::os::unix::fs::symlink(&entry.source, &entry.destination)?;
            #[cfg(windows)]
            std::os::windows::fs::symlink_dir(&entry.source, &entry.destination).context("Cannot create symlink; enable Windows Developer Mode or explicitly select copy mode")?;
            #[cfg(not(any(unix, windows)))]
            bail!("Symlink installation unsupported; explicitly select copy mode");
        }
        LinkMethod::Copy => {
            let staging = tempfile::Builder::new()
                .prefix(".uncle-skills-")
                .tempdir_in(
                    entry
                        .destination
                        .parent()
                        .context("Destination has no parent")?,
                )?;
            let content = staging.path().join("content");
            fs::create_dir(&content)?;
            for item in WalkDir::new(&entry.source)
                .follow_links(false)
                .min_depth(1)
                .sort_by_file_name()
            {
                let item = item?;
                let destination = content.join(item.path().strip_prefix(&entry.source)?);
                if item.file_type().is_symlink() {
                    bail!(
                        "Source changed during sync: symlink {}",
                        item.path().display()
                    );
                }
                if item.file_type().is_dir() {
                    fs::create_dir(&destination)?;
                } else if item.file_type().is_file() {
                    fs::copy(item.path(), &destination)?;
                } else {
                    bail!(
                        "Source changed during sync: special file {}",
                        item.path().display()
                    );
                }
            }
            if tree_hash(&content)? != entry.hash {
                bail!("Source changed during sync; retry after finishing edits");
            }
            if exists(&entry.destination)? {
                bail!(
                    "Destination appeared during sync: {}; preserve it and retry after resolving the conflict",
                    entry.destination.display()
                );
            }
            fs::rename(content, &entry.destination)?;
        }
    }
    Ok(())
}
fn save_state(path: &Path, state: &State) -> Result<()> {
    let mut temporary =
        tempfile::NamedTempFile::new_in(path.parent().context("State path has no parent")?)?;
    serde_json::to_writer_pretty(&mut temporary, state)?;
    temporary.write_all(b"\n")?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(path)
        .map_err(|error| error.error)
        .with_context(|| format!("Cannot atomically save {}", path.display()))?;
    Ok(())
}

/// Preflight all changes, then reconcile with an ownership journal. Identical
/// unowned installs remain unowned unless adoption is explicitly requested.
/// Dry runs never create files, directories, or locks.
pub fn sync(
    config: &Config,
    state_path: &Path,
    dry_run: bool,
    adopt_existing: bool,
) -> Result<Vec<String>> {
    absolute_clean(state_path)?;
    let wanted = desired(config, state_path)?;
    let _lock: Option<File> = if dry_run {
        None
    } else {
        fs::create_dir_all(state_path.parent().context("State path has no parent")?)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(state_path.with_extension("lock"))?;
        lock.try_lock_exclusive()
            .context("Another sync is running; wait for it to finish")?;
        Some(lock)
    };
    let mut state: State = if exists(state_path)? {
        serde_json::from_slice(&fs::read(state_path)?)
            .context("Cannot read ownership state; preserve and repair it before syncing")?
    } else {
        State {
            version: 1,
            entries: BTreeMap::new(),
        }
    };
    if state.version != 1 {
        bail!("Unsupported ownership state version {}", state.version);
    }
    for (path, entry) in &state.entries {
        if path != &entry.destination {
            bail!("Invalid ownership state: destination key mismatch");
        }
        absolute_clean(path)?;
        if exists(path)? && !matches_entry(entry)? {
            bail!(
                "Managed destination changed: {}; preserve edits and resolve manually",
                path.display()
            );
        }
    }
    let mut untouched = BTreeSet::new();
    let mut adopted = Vec::new();
    for (path, entry) in &wanted {
        if !state.entries.contains_key(path) && exists(path)? {
            if !matches_entry(entry)? {
                bail!(
                    "Unmanaged destination already exists: {}; preserve it and resolve explicitly",
                    path.display()
                );
            }
            if adopt_existing {
                adopted.push(entry.clone());
            } else {
                untouched.insert(path.clone());
            }
        }
    }
    let mut messages = Vec::new();
    for path in &untouched {
        messages.push(format!(
            "Leave identical unowned installation {} (use --adopt-existing to manage)",
            path.display()
        ));
    }
    for entry in adopted {
        messages.push(format!("Adopt {}", entry.destination.display()));
        if !dry_run {
            state.entries.insert(entry.destination.clone(), entry);
            save_state(state_path, &state)?;
        }
    }
    let previous: Vec<_> = state.entries.values().cloned().collect();
    for old in previous {
        if !wanted
            .get(&old.destination)
            .is_some_and(|next| same(&old, next))
        {
            messages.push(format!("Remove managed {}", old.destination.display()));
            if !dry_run {
                remove_owned(&old)?;
                state.entries.remove(&old.destination);
                save_state(state_path, &state)?;
            }
        }
    }
    for (path, entry) in wanted {
        if untouched.contains(&path) {
            continue;
        }
        if state
            .entries
            .get(&path)
            .is_some_and(|old| same(old, &entry))
            && exists(&path)?
        {
            continue;
        }
        // A dry-run adoption was not inserted into the live state above.
        if dry_run && adopt_existing && !state.entries.contains_key(&path) && exists(&path)? {
            continue;
        }
        messages.push(format!(
            "Install {:?} {} -> {}",
            entry.method,
            path.display(),
            entry.source.display()
        ));
        if !dry_run {
            state.entries.insert(path, entry.clone());
            save_state(state_path, &state)?;
            install(&entry).with_context(|| {
                format!(
                    "Cannot install {}; retry sync after resolving the error",
                    entry.destination.display()
                )
            })?;
        }
    }
    if !dry_run && !exists(state_path)? {
        save_state(state_path, &state)?;
    }
    Ok(messages)
}
