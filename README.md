# uncle-skills

Keep one local skills collection linked to your coding agents. Written in Rust.

**Early development:** GitHub builds test Linux, Windows, and macOS; Android
builds need native Termux smoke checks. See [testing](docs/testing.md) for the
current scope. This tool does not make every skill compatible with every agent.

## Install

Prebuilt release installation requires no Rust. The commands work after a
release with matching assets has been published:

```sh
curl -fsSL https://raw.githubusercontent.com/abutheone/uncle-skills-manager/main/install.sh | sh
```

PowerShell:

```powershell
& ([scriptblock]::Create((Invoke-WebRequest https://raw.githubusercontent.com/abutheone/uncle-skills-manager/main/install.ps1).Content))
```

You can download and inspect the scripts first, or manually unpack a matching
archive from Releases. Existing commands are preserved unless replacement is
explicitly requested. The installer prints the binary location and any PATH step.
It does not change shell startup files.

## First use

Start with a local Git checkout containing `skills/<name>/SKILL.md`:

```sh
uncle-skills agents
uncle-skills init --repo ~/.uncle-skills --agent codex --agent claude-code
uncle-skills sync --dry-run
uncle-skills sync
uncle-skills doctor
```

For a launcher that sets its own Claude config, add
`--claude-config-dir /path/to/claude-config` to `init`. An explicit
`--target claude-code=/path/to/skills` also works. The manager cannot see
environment variables set only inside a separate launcher.

On Windows, use `--method copy` when directory symlinks are unavailable. Modified
copies are protected from replacement. Edit source skills in the repository.

Enable dynamic zsh completion for the current shell with one line:

```sh
autoload -Uz compinit && compinit && source <(uncle-skills completions zsh)
```

Add that line to your shell configuration only if you want it loaded on startup.
Other completion scripts: `uncle-skills completions bash|fish|powershell`.

## Everyday use

- Edit existing skills in the source repository; symlinks read the same files.
- After creating, renaming, or deleting a source skill, run `uncle-skills sync`.
- Run `uncle-skills pull` to fast-forward a clean checkout and reconcile links.
- Commit and push with Git when you want to publish your local edits.
- Reload or restart agents if their skill lists do not reflect changes.

Existing unrelated skills and Claude's `synced/` directory are left alone. Use
`sync --adopt-existing` only to explicitly manage identical preexisting links or
copies. No skill scripts are executed by synchronization.

[Configuration and behavior](docs/usage.md) · [Development and tests](docs/testing.md)

```text
src/          CLI, local configuration, deployment engine
tests/       disposable filesystem and command tests
.github/     hosted tests and binary builds
docs/        focused usage and verification guidance
```
