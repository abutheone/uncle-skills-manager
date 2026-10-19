# Build and test without compiling on the phone

GitHub-hosted machines do compilation. The phone only downloads a finished
Android executable and runs short smoke checks. No local Rust build is needed.

## What the workflows do

`CI` runs formatting, Clippy, and all Rust tests on Ubuntu 24.04, Windows, and
macOS 14. Tests must use temporary skill repositories and explicit configuration
paths, never the developer's live agent directories. Installer syntax is checked
on the corresponding shells. Linux additionally runs `python3 tests/installers.py`: 
network-free archive installation, explicit replacement, corrupt-checksum refusal,
directory and broken-link protection, Android selection, and musl rejection. Repository write permission and user secrets are
not given to test jobs. Linux also runs a real interactive zsh test for flags,
subcommands, and newly created skill names. The test excludes insecure host
completion directories and leaves zsh security checks enabled.

`Build binaries` compiles release binaries and creates archives with SHA-256
files on relevant main-branch pushes, version tags, or manual dispatch:

| Target | Hosted machine | Execution coverage |
| --- | --- | --- |
| Linux x64 glibc | Ubuntu 24.04 | Native Rust tests in CI |
| Android ARM64, API 24+ | Ubuntu 24.04 with hosted Android NDK | Cross-compilation only; native Termux checks required |
| Windows x64 | Windows | Native Rust tests in CI |
| macOS ARM64 | macOS 14 | Native Rust tests in CI |

The Android build uses `ANDROID_NDK_LATEST_HOME` (or `ANDROID_NDK_HOME`) and
`aarch64-linux-android24-clang`, and fails explicitly when the hosted NDK is
missing. It does not confuse Android with a generic Linux ARM64 target.

Each job has a timeout and two Cargo build workers. Concurrent obsolete runs
are cancelled, build artifacts expire after three days, and no compilation cache
is uploaded. Public-repository standard hosted runners are the intended free
build route. Do not enable paid larger runners or private-repository overages.
Actions policies, artifact/storage allowances, and pricing can change; check the
account's billing settings rather than assuming this enforces a monetary cap.

Third-party actions are pinned to upstream commit SHAs, with their version or
branch documented beside each pin. Review pins periodically. Rust stable and
hosted runner contents still advance; action pinning does not pin the entire
build environment.

## Local verification versus supported platforms

Adding a workflow does not mean it has passed. Consult the actual Actions run
and record native Termux checks before claiming Android support. A working
symlink does not prove an agent loaded the skill; verify actual discovery too.

Before release, test additions, edits, deletion, repeated sync, custom Claude
configuration, unrelated existing files, broken links, case collisions, copied
file changes, dry-run, and Git failure states. Verify Windows both with and
without directory symlink permission. Hosted macOS/Windows tests do not cover
WSL, every shell launcher, web runtimes, or installed agents' live discovery.

After downloading the Android build artifact on Termux, extract the archive to
a disposable directory and run only short checks:

```sh
./uncle-skills --help
./uncle-skills --version
```

Then use a temporary fixture repo/config for init, dry-run, sync and doctor.
Do not aim an experimental build at real Codex/Claude directories. The fixture
must exercise the same symlink lifecycle without touching live skills.

## Releases and binary installation

Tags build assets; they do not publish a release. After checking every CI job,
download the four target artifacts and attach each archive and matching
`.sha256` file to a GitHub release for that tag. Until the first such release,
the install commands below will fail clearly with HTTP 404.

No automatic installer or workflow authenticates to the private skills repo.
The user configures ordinary Git credentials separately.

Default release repository: `abutheone/uncle-skills-manager`. Both installers
contain that constant; change it deliberately for a fork. No arbitrary download
host override is accepted. Unix release selection uses `UNCLE_SKILLS_VERSION`
(default `latest`); destination uses `UNCLE_SKILLS_BIN_DIR` (default
`$HOME/.local/bin`). PowerShell exposes `-Version` and `-InstallDir`.

One line for Unix/Termux, after reviewing the script:

```sh
curl --fail --location --proto '=https' https://raw.githubusercontent.com/abutheone/uncle-skills-manager/main/install.sh | sh
```

One line for PowerShell, after reviewing the script:

```powershell
& ([scriptblock]::Create((Invoke-WebRequest https://raw.githubusercontent.com/abutheone/uncle-skills-manager/main/install.ps1).Content))
```

For stricter review, download the script, inspect it, then execute the saved
file. The convenience commands execute current main-branch installer code;
release pinning alone does not pin that script. Neither installer changes shell
profiles, PATH persistently, execution policy, agent directories, or skills.
They print PATH guidance and require no Rust compiler or administrator access.
An existing command is preserved by default. To update or explicitly replace
one after reviewing it, save the Unix installer and run `sh install.sh --replace`,
or run the saved PowerShell installer with `-Replace`. The installers never
execute an existing command to infer ownership. A directory at the destination
is always refused.

Archive checksums downloaded from the same HTTPS release detect corruption and
mismatches. They do not independently authenticate a compromised repository or
publisher. Signing/attestations are a future enhancement, not a current promise.

To uninstall, remove the installed executable only. Removing the executable
does not remove skills or agent links.

## Optional lightweight Linux check

Do not install PRoot or a distro automatically. It consumes phone resources and
is not a security boundary. A manually installed Alpine environment is useful
for future musl testing, but the current Linux binary requires glibc: the
installer rejects Alpine/musl. Use native Termux checks first. Add a musl build
and actual Alpine tests before advertising Alpine installation support.

## Primary references

- [GitHub-hosted runners](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)
- [Ubuntu 24.04 hosted software and NDK variables](https://github.com/actions/runner-images/blob/main/images/ubuntu/Ubuntu2404-Readme.md)
- [Android NDK with other build systems](https://developer.android.com/ndk/guides/other_build_systems)
- [GitHub Actions billing](https://docs.github.com/en/billing/concepts/product-billing/github-actions)
