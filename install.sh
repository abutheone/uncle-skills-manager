#!/bin/sh
# Release binaries: abutheone/uncle-skills-manager. No Rust required.
set -eu
replace=no
case "${1:-}" in
  '') ;;
  --replace) replace=yes; shift ;;
  *) echo 'Usage: sh install.sh [--replace]' >&2; exit 1 ;;
esac
[ "$#" -eq 0 ] || { echo 'Usage: sh install.sh [--replace]' >&2; exit 1; }
repo=abutheone/uncle-skills-manager
version=${UNCLE_SKILLS_VERSION:-latest}
install_dir=${UNCLE_SKILLS_BIN_DIR:-"$HOME/.local/bin"}
case "$version" in latest|v[0-9]*) ;; *) echo 'Version must be latest or a v-prefixed release tag' >&2; exit 1;; esac
case "$version" in *[!a-zA-Z0-9._-]*) echo 'Invalid release tag' >&2; exit 1;; esac
for tool in curl tar awk; do command -v "$tool" >/dev/null 2>&1 || { echo "Missing required command: $tool" >&2; exit 1; }; done
arch=$(uname -m)
os=$(uname -s)
# Android also reports Linux; inspect Android before selecting a glibc binary.
if [ -n "${TERMUX_VERSION:-}" ] || [ -n "${ANDROID_ROOT:-}" ]; then os=Android; fi
case "$os/$arch" in
  Android/aarch64|Android/arm64) target=aarch64-linux-android ;;
  Linux/x86_64|Linux/amd64)
    getconf GNU_LIBC_VERSION >/dev/null 2>&1 || { echo 'Linux release requires glibc; Alpine/musl is not supported yet' >&2; exit 1; }
    target=x86_64-unknown-linux-gnu ;;
  Darwin/arm64|Darwin/aarch64) target=aarch64-apple-darwin ;;
  *) echo "No prebuilt binary for $os/$arch. See the repository support matrix." >&2; exit 1 ;;
esac
destination="$install_dir/uncle-skills"
if [ -e "$destination" ] || [ -L "$destination" ]; then
  [ "$replace" = yes ] || { echo "Destination already exists: $destination. Re-run with --replace to replace it explicitly." >&2; exit 1; }
  [ ! -d "$destination" ] || { echo 'Destination is a directory; nothing installed' >&2; exit 1; }
fi
asset="uncle-skills-$target.tar.gz"
if [ "$version" = latest ]; then base="https://github.com/$repo/releases/latest/download"; else base="https://github.com/$repo/releases/download/$version"; fi
work=$(mktemp -d)
staged=
trap 'rm -rf "$work"; if [ -n "$staged" ]; then rm -f "$staged"; fi' EXIT
trap 'exit 1' HUP INT TERM
curl --fail --location --proto '=https' --tlsv1.2 "$base/$asset" -o "$work/$asset"
curl --fail --location --proto '=https' --tlsv1.2 "$base/$asset.sha256" -o "$work/$asset.sha256"
expected=$(awk -v file="$asset" '$2 == file {print $1}' "$work/$asset.sha256")
[ "${#expected}" -eq 64 ] || { echo 'Missing or invalid SHA-256 checksum' >&2; exit 1; }
if command -v sha256sum >/dev/null 2>&1; then actual=$(sha256sum "$work/$asset" | awk '{print $1}');
elif command -v shasum >/dev/null 2>&1; then actual=$(shasum -a 256 "$work/$asset" | awk '{print $1}');
else echo 'Install sha256sum or shasum first' >&2; exit 1; fi
[ "$actual" = "$expected" ] || { echo 'Checksum mismatch; nothing installed' >&2; exit 1; }
# Extract only the expected binary, not arbitrary archive paths.
tar -xzf "$work/$asset" -C "$work" uncle-skills
[ -f "$work/uncle-skills" ] && [ ! -L "$work/uncle-skills" ] || { echo 'Invalid archive binary' >&2; exit 1; }
mkdir -p "$install_dir"
[ ! -d "$install_dir/uncle-skills" ] || { echo 'Destination is a directory' >&2; exit 1; }
staged=$(mktemp "$install_dir/.uncle-skills.XXXXXX")
cp "$work/uncle-skills" "$staged"
chmod 755 "$staged"
# Check again after download so a new destination is not silently overwritten.
if [ "$replace" = no ] && { [ -e "$destination" ] || [ -L "$destination" ]; }; then
  echo 'Destination appeared during installation; nothing replaced' >&2; exit 1
fi
mv -f "$staged" "$destination"
staged=
printf 'Installed %s/uncle-skills\n' "$install_dir"
printf 'If needed, add this directory to PATH: %s\n' "$install_dir"
printf 'No shell configuration files were modified. Run: uncle-skills --help\n'
