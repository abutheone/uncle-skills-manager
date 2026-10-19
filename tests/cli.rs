use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use tempfile::TempDir;

fn run(config: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_uncle-skills"))
        .arg("--config")
        .arg(config)
        .args(args)
        .output()
        .unwrap()
}
fn fixture() -> (
    TempDir,
    std::path::PathBuf,
    std::path::PathBuf,
    std::path::PathBuf,
) {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("source repo");
    let skill = repo.join("skills/research");
    fs::create_dir_all(&skill).unwrap();
    fs::write(
        skill.join("SKILL.md"),
        "---\nname: research\ndescription: Research provided topics.\n---\nUse primary sources.\n",
    )
    .unwrap();
    let cfg = temp.path().join("settings/config.toml");
    let dest = temp.path().join("custom claude/skills");
    (temp, repo, cfg, dest)
}
#[test]
fn cli_init_preview_deploy_and_remove_preserve_unrelated_content() {
    let (_tmp, repo, cfg, dest) = fixture();
    let target = format!("claude-code={}", dest.display());
    let out = run(
        &cfg,
        &[
            "init",
            "--repo",
            repo.to_str().unwrap(),
            "--target",
            &target,
            "--method",
            "copy",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!dest.exists());
    let before = fs::read(&cfg).unwrap();
    let out = run(
        &cfg,
        &[
            "init",
            "--repo",
            repo.to_str().unwrap(),
            "--target",
            &target,
            "--method",
            "copy",
        ],
    );
    assert!(!out.status.success());
    assert_eq!(fs::read(&cfg).unwrap(), before);
    assert!(run(&cfg, &["sync", "--dry-run"]).status.success());
    assert!(!dest.exists());
    fs::create_dir_all(dest.join("synced")).unwrap();
    fs::write(dest.join("synced/keep.txt"), "agent owned").unwrap();
    assert!(run(&cfg, &["sync"]).status.success());
    assert!(dest.join("research/SKILL.md").exists());
    assert!(run(&cfg, &["doctor"]).status.success());
    let show = run(&cfg, &["show", "research"]);
    assert!(show.status.success());
    assert!(String::from_utf8_lossy(&show.stdout).contains("Use primary sources"));
    assert!(!run(&cfg, &["show", "../settings"]).status.success());
    fs::remove_dir_all(repo.join("skills/research")).unwrap();
    assert!(run(&cfg, &["sync"]).status.success());
    assert!(!dest.join("research").exists());
    assert_eq!(
        fs::read_to_string(dest.join("synced/keep.txt")).unwrap(),
        "agent owned"
    );
}
#[test]
fn pull_refuses_uncommitted_changes_without_losing_files() {
    let (_tmp, repo, cfg, dest) = fixture();
    let target = format!("codex={}", dest.display());
    let out = run(
        &cfg,
        &[
            "init",
            "--repo",
            repo.to_str().unwrap(),
            "--target",
            &target,
            "--method",
            "copy",
        ],
    );
    assert!(out.status.success());
    assert!(
        Command::new("git")
            .arg("init")
            .arg(&repo)
            .output()
            .unwrap()
            .status
            .success()
    );
    let before = fs::read(repo.join("skills/research/SKILL.md")).unwrap();
    let out = run(&cfg, &["pull"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("local changes"));
    assert_eq!(
        fs::read(repo.join("skills/research/SKILL.md")).unwrap(),
        before
    );
    assert!(!dest.exists());
}
#[test]
fn completions_do_not_require_configuration() {
    let tmp = tempfile::tempdir().unwrap();
    for shell in ["zsh", "bash", "fish", "powershell"] {
        let out = run(&tmp.path().join("missing.toml"), &["completions", shell]);
        assert!(
            out.status.success(),
            "{shell}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(!out.stdout.is_empty());
        assert!(!tmp.path().join("missing.toml").exists());
    }
}

#[test]
fn pull_does_not_update_an_enclosing_repository() {
    let (temporary, repo, cfg, dest) = fixture();
    assert!(
        Command::new("git")
            .arg("init")
            .arg(temporary.path())
            .output()
            .unwrap()
            .status
            .success()
    );
    let target = format!("codex={}", dest.display());
    assert!(
        run(
            &cfg,
            &[
                "init",
                "--repo",
                repo.to_str().unwrap(),
                "--target",
                &target,
                "--method",
                "copy"
            ]
        )
        .status
        .success()
    );
    let output = run(&cfg, &["pull"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("enclosing repository"));
}
