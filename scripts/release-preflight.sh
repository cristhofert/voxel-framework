#!/usr/bin/env bash

set -Eeuo pipefail

usage() {
    printf 'Usage: %s <version>\n' "${0##*/}" >&2
    printf 'Example: %s 0.8.0\n' "${0##*/}" >&2
}

if [[ $# -ne 1 ]]; then
    usage
    exit 2
fi

release_version=$1
if [[ ! $release_version =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z]+([.-][0-9A-Za-z]+)*)?(\+[0-9A-Za-z]+([.-][0-9A-Za-z]+)*)?$ ]]; then
    printf 'error: version must be an unprefixed semantic version (for example, 0.8.0)\n' >&2
    exit 2
fi

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

for required_command in cargo git python3 rustc; do
    if ! command -v "$required_command" >/dev/null 2>&1; then
        printf 'error: required command not found: %s\n' "$required_command" >&2
        exit 1
    fi
done

allow_dirty=${RELEASE_PREFLIGHT_ALLOW_DIRTY:-0}
if [[ $allow_dirty != 0 && $allow_dirty != 1 ]]; then
    printf 'error: RELEASE_PREFLIGHT_ALLOW_DIRTY must be 0 or 1\n' >&2
    exit 2
fi

if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    printf 'error: release preflight must run from a Git working tree\n' >&2
    exit 1
fi

if ! worktree_status=$(git status --porcelain=v1 --untracked-files=normal); then
    printf 'error: unable to determine whether the working tree is clean\n' >&2
    exit 1
fi

if [[ $allow_dirty == 0 && -n $worktree_status ]]; then
    printf 'error: release preflight requires a clean working tree\n' >&2
    printf 'Commit or stash changes, then run the command again.\n' >&2
    exit 1
fi

if [[ $allow_dirty == 1 && -n $worktree_status ]]; then
    printf 'warning: dirty-tree protection disabled for preflight development/testing\n' >&2
    printf 'Never use RELEASE_PREFLIGHT_ALLOW_DIRTY=1 for an actual release.\n' >&2
fi

package_info=$(
    cargo metadata --locked --offline --no-deps --format-version 1 |
        python3 -c '
import json
import sys

metadata = json.load(sys.stdin)
packages = [package for package in metadata["packages"] if package["name"] == "voxel-framework"]
if len(packages) != 1:
    raise SystemExit(f"error: expected exactly one voxel-framework package, found {len(packages)}")
package = packages[0]
print(package["version"], package.get("rust_version") or "", sep="\t")
'
)
IFS=$'\t' read -r package_version rust_version <<<"$package_info"

if [[ $package_version != "$release_version" ]]; then
    printf 'error: requested version %s does not match Cargo package version %s\n' \
        "$release_version" "$package_version" >&2
    exit 1
fi

python3 - "$release_version" CHANGELOG.md <<'PY'
from datetime import date
from pathlib import Path
import re
import sys

version = sys.argv[1]
changelog = Path(sys.argv[2])
pattern = re.compile(rf"^## \[{re.escape(version)}\] - (\d{{4}}-\d{{2}}-\d{{2}})$")
matches = []
dated_release_headings = []
lines = changelog.read_text(encoding="utf-8").splitlines()

for line_number, line in enumerate(lines, start=1):
    if re.fullmatch(r"## \[[^]]+\] - \d{4}-\d{2}-\d{2}", line):
        dated_release_headings.append((line_number, line))
    match = pattern.fullmatch(line)
    if match:
        matches.append((line_number, match.group(1)))

if len(matches) != 1:
    raise SystemExit(
        f"error: expected exactly one '## [{version}] - YYYY-MM-DD' heading in {changelog}; "
        f"found {len(matches)}"
    )

line_number, release_date = matches[0]
if not dated_release_headings or dated_release_headings[0][0] != line_number:
    first_heading = dated_release_headings[0][1] if dated_release_headings else "none"
    raise SystemExit(
        f"error: {changelog}:{line_number} is not the newest dated release heading; "
        f"first dated release heading is {first_heading!r}"
    )

try:
    date.fromisoformat(release_date)
except ValueError as error:
    raise SystemExit(
        f"error: invalid release date in {changelog}:{line_number}: {release_date}"
    ) from error

section_end = next(
    (index for index in range(line_number, len(lines)) if lines[index].startswith("## ")),
    len(lines),
)
section = lines[line_number:section_end]
if not any(line.strip() and not line.startswith("### ") for line in section):
    raise SystemExit(
        f"error: release section in {changelog}:{line_number} contains no release notes"
    )
PY

run() {
    printf '\n+ '
    printf '%q ' "$@"
    printf '\n'
    "$@"
}

printf 'Release preflight for voxel-framework %s\n' "$release_version"
printf 'Rust MSRV declared by Cargo.toml: %s\n' "${rust_version:-not set}"
run rustc --version
run cargo --version

# Keep this list as the release source of truth. The manually dispatched workflow
# invokes this script rather than maintaining a second copy of the checklist.
run cargo fmt --all -- --check
run cargo check --locked --all-targets
run cargo check --locked --no-default-features
run cargo check --locked --all-features --all-targets
run cargo test --locked
run cargo test --locked --no-default-features
run cargo test --locked --all-features
run cargo test --locked --examples
# Verify this package line on its exact declared minimum, not just stable.
# The 1.89.0 toolchain must already be installed; do not silently substitute it.
run rustc +1.89.0 --version
run cargo +1.89.0 check --locked --lib
run cargo +1.89.0 check --locked --lib --no-default-features
run cargo +1.89.0 check --locked --lib --all-features
run cargo +1.89.0 test --locked --lib --no-default-features --features items,crafting items::inventory::tests
run cargo +1.89.0 test --locked --lib --no-default-features --features items,crafting items::persistence::tests
run cargo +1.89.0 test --locked --lib --no-default-features --features items,crafting crafting::tests
run cargo doc --locked --no-deps --all-features

package_dirty_args=()
if [[ $allow_dirty == 1 ]]; then
    package_dirty_args+=(--allow-dirty)
fi
run cargo package --locked --offline --list "${package_dirty_args[@]}"
run cargo package --locked --offline --no-verify "${package_dirty_args[@]}"

printf '\nRelease preflight passed for voxel-framework %s.\n' "$release_version"
printf 'Local gates only: targeted Windows inventory-persistence CI must also pass on this exact candidate ref.\n'
printf 'No crate was published, no tag was created, and no GitHub Release was created.\n'
