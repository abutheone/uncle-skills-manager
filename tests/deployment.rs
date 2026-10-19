use std::{fs, path::PathBuf};
use tempfile::TempDir;
use uncle_skills_manager::engine::{AgentTarget, Config, LinkMethod, discover_skills, sync};

struct Fixture {
    _temporary: TempDir,
    config: Config,
    state: PathBuf,
    target: PathBuf,
}
impl Fixture {
    fn new(method: LinkMethod) -> Self {
        let temporary = tempfile::tempdir().unwrap();
        // macOS temp paths can contain symlinked /var ancestors.
        let root = fs::canonicalize(temporary.path()).unwrap();
        let repo = root.join("repository");
        fs::create_dir_all(repo.join("skills")).unwrap();
        let target = root.join("agent/skills");
        let config = Config {
            version: 1,
            repo,
            agents: vec![AgentTarget {
                id: "test-agent".into(),
                skills_dir: target.clone(),
                method,
                skills: None,
            }],
        };
        let state = root.join("manager/state.json");
        Self {
            _temporary: temporary,
            config,
            state,
            target,
        }
    }
    fn skill(&self, name: &str) {
        let path = self.config.repo.join("skills").join(name);
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("SKILL.md"), format!("---\nname: {name}\ndescription: A useful example skill.\n---\nFollow these instructions.\n")).unwrap();
    }
    fn sync(&self) -> anyhow::Result<Vec<String>> {
        sync(&self.config, &self.state, false, false)
    }
}

#[test]
fn dry_run_does_not_create_destinations_state_or_lock() {
    let f = Fixture::new(LinkMethod::Copy);
    f.skill("research");
    let actions = sync(&f.config, &f.state, true, false).unwrap();
    assert_eq!(actions.len(), 1);
    assert!(!f.target.exists());
    assert!(!f.state.parent().unwrap().exists());
}

#[test]
fn copy_lifecycle_preserves_unrelated_content_and_is_idempotent() {
    let f = Fixture::new(LinkMethod::Copy);
    f.skill("research");
    fs::create_dir_all(f.target.join("synced")).unwrap();
    fs::write(f.target.join("synced/user.txt"), "keep").unwrap();
    assert_eq!(f.sync().unwrap().len(), 1);
    assert!(f.sync().unwrap().is_empty());
    fs::write(f.config.repo.join("skills/research/new.txt"), "updated").unwrap();
    assert_eq!(f.sync().unwrap().len(), 2);
    assert_eq!(
        fs::read_to_string(f.target.join("research/new.txt")).unwrap(),
        "updated"
    );
    fs::remove_dir_all(f.config.repo.join("skills/research")).unwrap();
    assert_eq!(f.sync().unwrap().len(), 1);
    assert!(!f.target.join("research").exists());
    assert_eq!(
        fs::read_to_string(f.target.join("synced/user.txt")).unwrap(),
        "keep"
    );
}

#[test]
fn modified_copy_blocks_all_other_changes() {
    let f = Fixture::new(LinkMethod::Copy);
    f.skill("research");
    f.sync().unwrap();
    fs::write(f.target.join("research/SKILL.md"), "local edit").unwrap();
    f.skill("another");
    assert!(
        f.sync()
            .unwrap_err()
            .to_string()
            .contains("Managed destination changed")
    );
    assert!(!f.target.join("another").exists());
    assert_eq!(
        fs::read_to_string(f.target.join("research/SKILL.md")).unwrap(),
        "local edit"
    );
}

#[test]
fn unmanaged_conflict_blocks_entire_plan() {
    let f = Fixture::new(LinkMethod::Copy);
    f.skill("a-first");
    f.skill("z-conflict");
    fs::create_dir_all(f.target.join("z-conflict")).unwrap();
    fs::write(f.target.join("z-conflict/precious"), "preserve").unwrap();
    assert!(f.sync().is_err());
    assert!(!f.target.join("a-first").exists());
    assert!(f.target.join("z-conflict/precious").exists());
}

#[test]
fn identical_unowned_copy_is_not_adopted_without_explicit_flag() {
    let f = Fixture::new(LinkMethod::Copy);
    f.skill("research");
    fs::create_dir_all(f.target.join("research")).unwrap();
    fs::copy(
        f.config.repo.join("skills/research/SKILL.md"),
        f.target.join("research/SKILL.md"),
    )
    .unwrap();
    assert!(f.sync().unwrap()[0].contains("unowned"));
    fs::remove_dir_all(f.config.repo.join("skills/research")).unwrap();
    assert!(f.sync().unwrap().is_empty());
    assert!(f.target.join("research").exists());
}

#[test]
fn explicit_adoption_allows_future_managed_removal() {
    let f = Fixture::new(LinkMethod::Copy);
    f.skill("research");
    fs::create_dir_all(f.target.join("research")).unwrap();
    fs::copy(
        f.config.repo.join("skills/research/SKILL.md"),
        f.target.join("research/SKILL.md"),
    )
    .unwrap();
    let preview = sync(&f.config, &f.state, true, true).unwrap();
    assert_eq!(preview.len(), 1);
    assert!(preview[0].starts_with("Adopt"));
    assert!(!f.state.exists());
    sync(&f.config, &f.state, false, true).unwrap();
    fs::remove_dir_all(f.config.repo.join("skills/research")).unwrap();
    f.sync().unwrap();
    assert!(!f.target.join("research").exists());
}

#[test]
fn selected_skills_can_disable_without_deleting_sources() {
    let mut f = Fixture::new(LinkMethod::Copy);
    f.skill("research");
    f.skill("review");
    f.config.agents[0].skills = Some(vec!["research".into()]);
    f.sync().unwrap();
    assert!(f.target.join("research").exists());
    assert!(!f.target.join("review").exists());
    f.config.agents[0].skills = Some(vec![]);
    f.sync().unwrap();
    assert!(!f.target.join("research").exists());
    assert!(f.config.repo.join("skills/research/SKILL.md").exists());
}

#[test]
fn metadata_errors_and_unknown_selections_fail_before_writes() {
    let mut f = Fixture::new(LinkMethod::Copy);
    f.skill("research");
    fs::write(
        f.config.repo.join("skills/research/SKILL.md"),
        "---\nname: wrong\ndescription: Example\n---\n",
    )
    .unwrap();
    assert!(f.sync().is_err());
    assert!(!f.target.exists());
    f.skill("research");
    f.config.agents[0].skills = Some(vec!["missing".into()]);
    assert!(f.sync().is_err());
    assert!(!f.target.exists());
}

#[test]
fn destination_cannot_overlap_repository_or_state() {
    let mut f = Fixture::new(LinkMethod::Copy);
    f.skill("research");
    f.config.agents[0].skills_dir = f.config.repo.join("skills");
    assert!(f.sync().is_err());
    f.config.agents[0].skills_dir = f.state.parent().unwrap().to_owned();
    assert!(f.sync().is_err());
}

#[test]
fn missing_managed_destination_recovers_from_install_intent() {
    let f = Fixture::new(LinkMethod::Copy);
    f.skill("research");
    f.sync().unwrap();
    fs::remove_dir_all(f.target.join("research")).unwrap();
    assert_eq!(f.sync().unwrap().len(), 1);
    assert!(f.target.join("research/SKILL.md").exists());
}

#[test]
fn invalid_source_tree_is_not_treated_as_deletion() {
    let f = Fixture::new(LinkMethod::Copy);
    f.skill("research");
    f.sync().unwrap();
    fs::remove_dir_all(f.config.repo.join("skills")).unwrap();
    assert!(f.sync().is_err());
    assert!(f.target.join("research/SKILL.md").exists());
}

#[test]
fn source_rename_removes_only_previous_install() {
    let f = Fixture::new(LinkMethod::Copy);
    f.skill("research");
    f.sync().unwrap();
    fs::remove_dir_all(f.config.repo.join("skills/research")).unwrap();
    f.skill("investigate");
    f.sync().unwrap();
    assert!(!f.target.join("research").exists());
    assert!(f.target.join("investigate/SKILL.md").exists());
}

#[cfg(unix)]
#[test]
fn symlink_source_edits_and_deleted_source_cleanup_work() {
    let f = Fixture::new(LinkMethod::Symlink);
    f.skill("research");
    f.sync().unwrap();
    assert!(
        fs::symlink_metadata(f.target.join("research"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    fs::write(f.config.repo.join("skills/research/file.txt"), "live edit").unwrap();
    assert_eq!(
        fs::read_to_string(f.target.join("research/file.txt")).unwrap(),
        "live edit"
    );
    assert!(f.sync().unwrap().is_empty());
    fs::remove_dir_all(f.config.repo.join("skills/research")).unwrap();
    assert_eq!(f.sync().unwrap().len(), 1);
    assert!(fs::symlink_metadata(f.target.join("research")).is_err());
}

#[cfg(unix)]
#[test]
fn external_source_symlinks_and_symlinked_skills_root_are_rejected() {
    let f = Fixture::new(LinkMethod::Copy);
    f.skill("research");
    std::os::unix::fs::symlink(
        f.config.repo.join("skills/research/SKILL.md"),
        f.config.repo.join("skills/research/escape"),
    )
    .unwrap();
    assert!(discover_skills(&f.config.repo).is_err());
    assert!(!f.target.exists());
    fs::remove_dir_all(f.config.repo.join("skills")).unwrap();
    let external = f.config.repo.parent().unwrap().join("external");
    fs::create_dir(&external).unwrap();
    std::os::unix::fs::symlink(external, f.config.repo.join("skills")).unwrap();
    assert!(discover_skills(&f.config.repo).is_err());
}

#[cfg(unix)]
#[test]
fn replacing_managed_link_with_other_target_is_not_deleted() {
    let f = Fixture::new(LinkMethod::Symlink);
    f.skill("research");
    f.sync().unwrap();
    fs::remove_file(f.target.join("research")).unwrap();
    std::os::unix::fs::symlink(f.config.repo.parent().unwrap(), f.target.join("research")).unwrap();
    assert!(f.sync().is_err());
    assert!(
        fs::symlink_metadata(f.target.join("research"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[test]
fn concurrent_sync_lock_fails_cleanly() {
    use fs2::FileExt;
    let f = Fixture::new(LinkMethod::Copy);
    f.skill("research");
    fs::create_dir_all(f.state.parent().unwrap()).unwrap();
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(f.state.with_extension("lock"))
        .unwrap();
    lock.try_lock_exclusive().unwrap();
    assert!(f.sync().unwrap_err().to_string().contains("Another sync"));
    assert!(!f.target.exists());
}

#[test]
fn codex_and_pi_can_share_one_managed_directory() {
    let mut f = Fixture::new(LinkMethod::Copy);
    f.skill("research");
    let mut second = f.config.agents[0].clone();
    second.id = "pi".into();
    f.config.agents.push(second);
    assert_eq!(f.sync().unwrap().len(), 1);
    assert!(f.sync().unwrap().is_empty());
    f.config.agents.pop();
    assert!(f.sync().unwrap().is_empty());
    f.config.agents.push(AgentTarget {
        id: "pi".into(),
        skills_dir: f.target.clone(),
        method: LinkMethod::Copy,
        skills: Some(vec![]),
    });
    assert!(f.sync().is_err());
    assert!(f.target.join("research/SKILL.md").exists());
}

#[test]
fn crlf_metadata_is_supported_and_unsafe_names_are_rejected() {
    let f = Fixture::new(LinkMethod::Copy);
    f.skill("research");
    fs::write(
        f.config.repo.join("skills/research/SKILL.md"),
        "---\r\nname: research\r\ndescription: Useful research\r\n---\r\nInstructions\r\n",
    )
    .unwrap();
    assert_eq!(discover_skills(&f.config.repo).unwrap(), vec!["research"]);
    fs::create_dir(f.config.repo.join("skills/Bad_Name")).unwrap();
    assert!(discover_skills(&f.config.repo).is_err());
}

#[test]
fn explicit_method_change_replaces_only_unchanged_managed_install() {
    let mut f = Fixture::new(LinkMethod::Copy);
    f.skill("research");
    f.sync().unwrap();
    // Replacing the method is allowed only for unchanged, state-owned content.
    f.config.agents[0].method = LinkMethod::Symlink;
    let actions = sync(&f.config, &f.state, true, false).unwrap();
    assert_eq!(actions.len(), 2);
    assert!(actions[0].starts_with("Remove"));
    assert!(actions[1].starts_with("Install"));
    assert!(f.target.join("research/SKILL.md").exists());
}

#[test]
fn metadata_without_instruction_body_is_rejected() {
    let f = Fixture::new(LinkMethod::Copy);
    f.skill("research");
    for ending in ["---", "---\n", "---\n \n\t"] {
        fs::write(
            f.config.repo.join("skills/research/SKILL.md"),
            format!("---\nname: research\ndescription: Example\n{ending}"),
        )
        .unwrap();
        assert!(
            discover_skills(&f.config.repo)
                .unwrap_err()
                .to_string()
                .contains("nonempty instructions")
        );
    }
    assert!(!f.target.exists());
}

#[cfg(unix)]
#[test]
fn relative_unowned_link_can_be_adopted_then_removed_after_source_deletion() {
    let f = Fixture::new(LinkMethod::Symlink);
    f.skill("research");
    fs::create_dir_all(&f.target).unwrap();
    std::os::unix::fs::symlink(
        "../../repository/skills/research",
        f.target.join("research"),
    )
    .unwrap();
    assert!(f.sync().unwrap()[0].contains("unowned"));
    assert_eq!(sync(&f.config, &f.state, false, true).unwrap().len(), 1);
    assert!(f.sync().unwrap().is_empty());
    fs::remove_dir_all(f.config.repo.join("skills/research")).unwrap();
    assert_eq!(f.sync().unwrap().len(), 1);
    assert!(fs::symlink_metadata(f.target.join("research")).is_err());
}

#[test]
fn assets_larger_than_hash_buffer_copy_and_update_correctly() {
    let f = Fixture::new(LinkMethod::Copy);
    f.skill("research");
    let source = f.config.repo.join("skills/research/asset.bin");
    let mut content = vec![0x5a; 131_073];
    fs::write(&source, &content).unwrap();
    f.sync().unwrap();
    assert_eq!(
        fs::read(f.target.join("research/asset.bin")).unwrap(),
        content
    );
    content[65_536] = 0xff;
    fs::write(source, &content).unwrap();
    assert_eq!(f.sync().unwrap().len(), 2);
    assert_eq!(
        fs::read(f.target.join("research/asset.bin")).unwrap(),
        content
    );
}
