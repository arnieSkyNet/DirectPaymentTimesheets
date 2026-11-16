# Source delivery for binary releases

DirectPaymentTimesheets is GPL-3.0-or-later. Select GPL-compatible alternatives
for dual-licensed dependencies and retain all applicable notices. Vendored D-Bus
is GPL-2.0-or-later (use GPLv3 here); its Rust wrapper's licence is not its native
licence. The generated crate notice tree includes exact upstream manifests,
README attribution and nested native COPYING/LICENSE/NOTICE files.

`packaging/notice-allowlist.json` is a reviewed release input generated from the
checksum-verified archives and the explicit retained font/native notice list.
Regenerate it with `python3 scripts/third_party_licenses.py --write` when notices
or the lockfile change, review its diff, and run `--check`. Installer staging
checks exact filenames and hashes without needing archive downloads; unexpected
files (including Git-ignored files), symlinks and special files are refused.
Windows consumes the same verified list rather than recursively discovering files.
The manifest's notice maps must match this independent inventory exactly.

Before distributing binaries, prepare the exact reviewed clean source commit:

```sh
cargo fetch --locked
python3 scripts/third_party_licenses.py --check
python3 packaging/source_bundle.py --check
python3 packaging/source_bundle.py --output dist/corresponding-source.tar.gz
```

Creation refuses an uncommitted or untracked source change. It exports
application/build/packaging sources from the exact Git commit, checks each exported
file against its Git blob identity and requires complete tree coverage, and includes every checksum-verified locked `.crate`
archive, including native source embedded by libdbus-sys, libsqlite3-sys, Ring,
zstd-sys and other sys crates. It does not copy local Cargo configuration, Git
metadata, caches, application profiles, payroll data or ignored build evidence.
All crate archives include their original notices and preferred modification
sources. The bundle manifest binds the application commit and crate checksums. Archive
bytes are rechecked at inclusion; offline vendoring consumes the same verified
bytes rather than reopening a mutable archive path.

For an offline source build, unpack the bundle and run from its top directory:

```sh
python3 application/packaging/offline_vendor.py SOURCE-MANIFEST.json crates /tmp/dpts-vendor
cd application
cargo --offline --locked --config 'source.crates-io.replace-with="dpts-vendor"' --config 'source.dpts-vendor.directory="/tmp/dpts-vendor"' build --release
```

Use a new vendor destination and the release's pinned toolchain and required
platform development libraries. For installers, apply the same source-replacement
configuration to the existing platform scripts in RELEASE-BUILDS.md. The helper
verifies archive checksums, rejects links/traversal and creates Cargo's per-file
vendor checksums. Keep this setup separate from personal Cargo state. Source
consistency does not establish bit-for-bit reproducibility.

## Distribution-native source annex

The crate bundle alone is insufficient for non-system libraries copied into an
AppImage or another package. Inventory each actual copied library and runtime,
its distribution package/version/source package, patches, licence and required
build scripts. Retain matching source archives and applicable build instructions
in a companion annex, with SHA-256 checksums. Debian source packages must include
their distribution patches, not merely an upstream tarball. Verify the annex
against the actual package library inventory before binary distribution approval.
Keep the exact pinned AppImage runtime source available too. Do not treat a
copyright-file copy or a mutable homepage as delivery of corresponding source.

Offer the bundle and required annex from the same download location as the
binaries, at no extra charge, with clear source links beside each binary set
(GPLv3 section 6(d)). Preserve their availability while those binaries are
offered. Do not promise a section 6(b) written offer unless its separate duration
and fulfilment requirements will actually be met. No publisher is implemented or
activated by this document or script.

## Baseline and historical binaries

The ten locally verified clean-baseline packages identify commit
`995c3c8cf179dd522d595c20bbacce40a9b29304`, build run `43763642406`.
They predate the current remediation. They must not be described as containing
these fixes. Historical v1.0.7 packages attributed to `2ae393f` are a different
source set; the excluded private history has not been accessed or restored.
Retain honest historical provenance and obtain independently reviewed exact
source correspondence before offering historical binaries from this repository.

No matching remediated native package set or distribution-native source annex
has been created/accepted in this local remediation pass. Those are release gates.
