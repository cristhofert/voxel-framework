#!/usr/bin/env bash
# Local replacement for the GitHub Actions "Rust CI" workflow.
#
# Usage:
#   scripts/ci-local.sh          # quick: fmt, check all targets, cargo test
#   scripts/ci-local.sh --full   # everything .github/workflows/rust.yml runs
#
# --full also runs the MSRV lane when the 1.89.0 toolchain is installed
# (`rustup toolchain install 1.89.0`); otherwise it is skipped with a warning.
# Linux needs: pkg-config libasound2-dev libwayland-dev libudev-dev

set -Eeuo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

mode=quick
case "${1:-}" in
    "") ;;
    --full) mode=full ;;
    -h|--help) sed -n '2,10p' "$0"; exit 0 ;;
    *) printf 'error: unknown argument: %s\n' "$1" >&2; exit 2 ;;
esac

step() {
    printf '\n==> %s\n' "$*"
    "$@"
}

step cargo fmt --check
step cargo check --all-targets
step cargo test

if [[ $mode == quick ]]; then
    printf '\nQuick checks passed. Run with --full before a release or merge.\n'
    exit 0
fi

# Check matrix
step cargo check --no-default-features
step cargo check --all-features --all-targets

# Test matrix
step cargo test --no-default-features
step cargo test --all-features
step cargo test --examples

# Docs and package audit
step cargo doc --no-deps --all-features
step cargo package --list >/dev/null
step cargo package --no-verify --allow-dirty

# MSRV lane (library only, like CI)
if rustup toolchain list 2>/dev/null | grep -q '^1\.89\.0'; then
    step cargo +1.89.0 check --lib
    step cargo +1.89.0 check --lib --no-default-features
    step cargo +1.89.0 check --lib --all-features
    for filter in items::inventory::tests items::persistence::tests crafting::tests; do
        step cargo +1.89.0 test --lib --no-default-features --features items,crafting "$filter"
    done
else
    printf '\nwarning: Rust 1.89.0 not installed; skipping MSRV lane\n' >&2
fi

printf '\nAll CI checks passed locally.\n'
