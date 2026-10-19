# Configuration and behavior

`uncle-skills` manages an existing local repository. Clone private repositories
with your existing Git authentication first; the tool never stores tokens.
The default configuration uses your platform's user configuration directory.
Use `--config PATH` or `UNCLE_SKILLS_CONFIG` to select a different file.
`init` refuses to overwrite an existing configuration.

## Separate source and local state

Keep portable skill files in Git. Configuration paths and deployment state belong
on each machine, outside the skills repository. State is stored beside the
configuration as `config.state.json`; a local lock prevents overlapping syncs.
Do not copy another machine's deployment state.

The generated TOML has this shape (replace paths with absolute local paths):

```toml
version = 1
repo = "/path/to/skills-repository"

[[agents]]
id = "claude-code"
skills_dir = "/path/to/claude-config/skills"
method = "symlink"
# Omit skills to include all valid source skills. Use [] to disable this target.
# skills = ["research", "code-review"]
```

Edit local configuration to change selected skills or register another target;
preview with `sync --dry-run`. `--target AGENT=PATH` during init overrides defaults.
Known target IDs are codex, claude-code, pi, and gemini-cli. Claude targets
reject its reserved skill names `synced` and `anthropic-skills`. Agent detection is a
PATH check, not proof of authentication or runtime skill discovery.

Codex and Pi use the same default `.agents/skills` root. Identical shared targets
must use the same method and selection. Other agents may also read this shared
root: profiles select deployments, not an access-control boundary. Native agents
must be checked for their full discovery rules and duplicate-name behavior.

## Safe reconciliation

A sync validates all selected source skills and preflights conflicts before it
changes installations. It owns only entries recorded in its local state. An
identical unowned installation is preserved; explicit adoption is needed before
the manager can later remove it. Unrelated entries are never adopted implicitly.

Symlinks use canonical absolute source paths. Moving a checkout requires an
updated configuration and reconciliation. Copies carry a tree hash; changes to
a managed destination stop an update or removal instead of being overwritten.
Deleting a source skill removes only an unchanged manager-owned installation.
Deleting a link does not delete source files. Concurrent external edits during
sync are unsupported; close editors/agents that write these paths first.

Ownership intent is journaled before deployment changes. If interrupted, rerun
sync; it checks existing content before resuming. This is recoverable multi-file
work, not an atomic transaction over every agent directory. Keep the state file;
if it is lost, use a dry run and explicit adoption rather than deleting files.

Source skill folders and their resources must contain regular files/directories,
not symlinks or special devices. This intentionally stricter subset prevents
resource traversal outside the source tree. Metadata checks do not establish
script safety or compatibility of all nested filenames with every OS.

## Git and updates

`pull` refuses dirty checkouts and detached HEADs and uses `git pull --ff-only`.
If authentication, upstream configuration, or histories conflict, resolve with
Git and retry. It never auto-commits, force-pushes, stashes, or resets your work.
A post-pull validation conflict leaves the new checkout in place and reports the
problem; it does not pretend that both Git and deployment form one transaction.

The manager binary and skill content have separate update lifecycles. Binary
updates rerun the release installer with explicit replacement. Skills update
through Git plus reconciliation. No background network sync is installed.

## Verification

`doctor` checks files and configuration. It does not make model requests or prove
that an agent loaded the skill. Check the native skill list and invoke a harmless
example after setup. Cloud/web agents require their own supported deployment
mechanism; local links do not reach remote execution environments.

## References

- [Agent Skills specification](https://agentskills.io/specification)
- [Claude configuration overrides](https://code.claude.com/docs/en/claude-directory)
- [Claude skill discovery and synced content](https://code.claude.com/docs/en/skills)
- [Codex local discovery](https://learn.chatgpt.com/docs/build-skills)
- [Pi discovery](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/skills.md)
- [Gemini discovery](https://geminicli.com/docs/cli/skills/)
