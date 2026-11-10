# Releasing voxel-framework

Publishing remains a human-controlled operation. The automation in this
repository only validates a candidate release; it never publishes to crates.io,
creates or pushes a tag, or creates a GitHub Release.

## Prepare the release

1. Update the package version in `Cargo.toml`. `Cargo.lock` is ignored by this
   library: after all candidate workers finish, prepare a lockfile for the exact
   candidate and retain it as verification evidence. Install Rust **1.89.0**
   as well as stable before running the gate; the preflight does not substitute
   stable for the minimum toolchain. Dependency resolution on that exact minimum
   must be checked, including the optional `tempfile =3.27.0` used by `items`.
2. Add a dated `## [X.Y.Z] - YYYY-MM-DD` heading and release notes to
   `CHANGELOG.md`.
3. From a clean working tree, run the complete local gate:

   ```sh
   ./scripts/release-preflight.sh X.Y.Z
   ```

4. Require normal Rust CI, including **Windows inventory persistence (items
   only)**, on the same exact candidate ref. The local script checks the Linux
   feature matrix, exact Rust 1.89.0 default/no-default/all-features library
   checks and focused inventory/persistence/crafting tests, docs, and package
   contents; it cannot supply Windows execution evidence. Inspect native
   permission/symlink test exclusions rather than treating Linux results or a
   filtered zero-test run as Windows proof. Metadata, a dated changelog, or
   local preflight success alone does not make a candidate ready.
5. Any integration or publication is a separate human-controlled decision after
   the exact candidate gates pass. The **Release preflight** workflow may also
   be run manually for the candidate ref with the same unprefixed version; it
   calls the local script as
   its single source of truth. The workflow checks out without persisting GitHub
   credentials and has read-only repository permissions.

The validation commands use `Cargo.lock`, but the check, test, and documentation
steps may download the locked toolchain dependencies when they are not already
cached. Only the final package listing and package creation are forced offline;
they deliberately use the dependencies fetched by the earlier validation steps.
`RELEASE_PREFLIGHT_ALLOW_DIRTY=1` exists only to test changes to the preflight
itself and must never be used to approve a release.

For **0.9.0**, use `0.9.0` as the preflight version. The release changes native
player focus/capture behavior and public player-system signatures; keep the
CHANGELOG migration notes in the release announcement. Candidate work must stop
without pushing, tagging, publishing, or dispatching automation unless separately
authorized. Check registry/tag existence rather than inferring publication from
the manifest version.

## Publish and announce manually

From a clean, up-to-date checkout of the exact commit to release:

```sh
git switch dev
git pull --ff-only
./scripts/release-preflight.sh X.Y.Z
cargo publish --dry-run --locked
cargo publish --locked
```

Wait until crates.io shows the new version, then create and push the annotated
tag for the exact commit that was published:

```sh
git tag -a vX.Y.Z -m "Release vX.Y.Z"
git push origin vX.Y.Z
```

Finally, create the GitHub Release for `vX.Y.Z` manually and use the matching
`CHANGELOG.md` section as its notes. If publishing fails, do not create or push
the tag; fix the release preparation and rerun the preflight first.
