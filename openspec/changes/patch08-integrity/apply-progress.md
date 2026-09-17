# Apply Progress: patch08-integrity candidate 0.8.1

status: implementation_complete_verification_pending
candidate_ready: false

## Completed implementation
18/18 tasks complete: 1.1–1.9 inventory/crafting/prelude, 2.1–2.6 inventory persistence/dependencies, 3.1–3.3 candidate CI/docs. No forbidden module changes. P1 decoded-alias save validation and actual alias regressions implemented; P2 protected effective-user Windows private creation and fail-closed prewrite ACL/volume readback implemented with three no-rights-skip native tests. Independent final static review PASS, with no actionable remaining finding.

## Existing executed evidence (not rerun by preparation operator)
Evidence: /tmp/voxel-0.8.1-focused/after-fix/report.md and results.json. Independent static review PASS for P1/P2. Linux focused default tests: inventory 21, crafting 27, persistence 17 = 65 passed, zero failed/ignored. Actual Rust 1.89.0 default and no-default library checks passed. Stable Rust 1.94.1 Windows-GNU items-only library/test cross-check passed; three Windows security test bodies compiled, not executed or linked natively. All commands locked/offline, frozen dirty source and ignored lock unchanged. Lock SHA256: 0abec3ecfd7fe79db200d34e68b7956f4396d38c286de936176d09f7ba62e611. Evidence baseline HEAD: 1a82c7e66dfcadde7996de03c4d9ae6218ee1b2e.

## Pending formal verification
Tasks 4.1–4.5 remain unchecked. Full exact-candidate default/no-default/all-features check/test and examples; exact Rust 1.89.0 all-features/focused tests; native targeted Windows execution including three security tests; docs/package and clean 0.8.1 preflight are pending. Dirty focused evidence and cross-check are not clean-candidate or Windows runtime proof. Formal verifier owns verify-report.md.

## Authorized preparation operations
User separately authorized this implementation commit and one non-force upstream push of codex/patch08-integrity to origin solely for candidate Rust CI. Do not merge/push dev, create a PR/tag/release, publish or expand source scope. Workflow dispatch is allowed only once on this candidate ref if push did not trigger, after safely checking for existing matching runs; otherwise do not dispatch a duplicate. Commit/run identity will be reported externally and saved to Engram, avoiding self-referential tracked SHA or post-commit tree changes.
