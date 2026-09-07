# Releasing voxel-framework

Publishing remains a human-controlled operation. The automation in this
repository only validates a candidate release; it never publishes to crates.io,
creates or pushes a tag, or creates a GitHub Release.

## Prepare the release

1. Update the package version in `Cargo.toml` and let Cargo update the root
   package entry in `Cargo.lock`.
2. Add a dated `## [X.Y.Z] - YYYY-MM-DD` heading and release notes to
   `CHANGELOG.md`.
3. From a clean working tree, run the complete local gate:

   ```sh
   ./scripts/release-preflight.sh X.Y.Z
   ```

4. Open and merge the release preparation pull request after normal Rust CI is
   green. The **Release preflight** workflow may also be run manually for the
   candidate ref with the same unprefixed version; it calls the local script as
   its single source of truth. The workflow checks out without persisting GitHub
   credentials and has read-only repository permissions.

The validation commands use `Cargo.lock`, but the check, test, and documentation
steps may download the locked toolchain dependencies when they are not already
cached. Only the final package listing and package creation are forced offline;
they deliberately use the dependencies fetched by the earlier validation steps.
`RELEASE_PREFLIGHT_ALLOW_DIRTY=1` exists only to test changes to the preflight
itself and must never be used to approve a release.

## Publish and announce manually

From a clean, up-to-date checkout of the exact commit to release:

```sh
git switch main
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
