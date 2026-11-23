# Verification

See docs/verification/hotbar-item-icons.md for the concrete commands, results,
native geometry, observed failures/fixes, dependency-lock distinction, and release
limitations. Evidence lives in target/verification/hotbar-item-icons.

351 default-feature library tests pass, including 15 hotbar tests. The feature
and Rust 1.89 matrix passes. Native layout is verified at the three required
sizes plus 600x600. Full u32 text is preserved with font sizing rather than
changing stack limits. Package preparation does not authorize publication.

No consumer source or Cargo dependency source was edited. No push, tag, or publication has been performed. The maintainer subsequently
authorized a local commit of the verified candidate. The local SDD artifacts are authoritative here;
Engram writes are unavailable without a confirmed host runtime identity.
