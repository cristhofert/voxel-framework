## Summary

<!-- What changes and why. Keep it short — details below. -->

## SDD

<!-- Pick one. See docs/sdd/conventions.md for what counts as substantial. -->

- [ ] **Substantial change** — linked SDD change: `openspec/changes/<change-name>/`
- [ ] **Not substantial** — typo, internal refactor without API delta, doc tweak that tracks an accepted spec, asset-only update, semver-compatible dep bump, etc.

## Verification

- [ ] `cargo check` passes (and `cargo check --example basic_world` if examples touched)
- [ ] `cargo test` passes
- [ ] If public API or behavior changed, `README.md` and `CHANGELOG.md` are in sync with the accepted spec
- [ ] If the change touches a documented invariant, `docs/multiplayer-readiness.md` (or relevant audit doc) is reviewed

## Notes for the reviewer

<!-- Optional: tradeoffs, follow-ups, anything you want a second pair of eyes on. -->
