# Artifact-only packaging rehearsals

This infrastructure is a rehearsal, not a release publisher. The application is
still 1.0.1 and its schema is 32. No new downloadable release is claimed. All
package versions come from Cargo.toml; filenames below use 1.0.1 intentionally.

## Entry point and source identity

After the changes are reviewed, committed and pushed **by an authorised operator**,
run `.github/workflows/package-rehearsal.yml` (Multi-platform package rehearsal).
Its `target` input accepts `all`, `linux-x86_64`, `linux-armv7`, `linux-aarch64`,
`windows-x86_64`, `macos-arm64` or `macos-x86_64`. Start with `linux-armv7` to prove
the highest-risk path, then run `all` from the same reviewed revision.

The preparation job captures checkout HEAD and Cargo's package version. Every
reusable workflow checks out that exact SHA with persisted credentials disabled.
All workflows use `contents: read`, only manual/reusable triggers, and have **no
release publisher or tag/push trigger**. No GitHub Release can be created by these
jobs. The three platform workflows also accept independent manual dispatches for
diagnosis, but only the `all` orchestrator verifies a complete nine-package set.

Each package carries adjacent `.json` provenance and `.sha256` files. They record
source SHA, version, target, installation kind, tool pins and package digest.
An all-target run checks the exact package inventory, rejects duplicate/missing
packages, mixed revisions/versions and digest mismatches, flattens the packages,
and writes `rehearsal-manifest.json` and `SHA256SUMS`. Actions artifacts are retained
for 30 days. No package is uploaded as a release asset. A failed platform prevents
the complete artifact; successful independent jobs can still be inspected.

## Targets

| Workflow target | Runner | Rust target | Packages (VERSION from Cargo) |
|---|---|---|---|
| linux-x86_64 | ubuntu-24.04 | x86_64-unknown-linux-gnu | direct-payment-timesheets_VERSION_amd64.deb; DirectPaymentTimesheets-VERSION-linux-x86_64.AppImage |
| linux-armv7 | ubuntu-24.04, cross compiler + QEMU | armv7-unknown-linux-gnueabihf | direct-payment-timesheets_VERSION_armhf.deb; DirectPaymentTimesheets-VERSION-linux-armhf.AppImage |
| linux-aarch64 | ubuntu-24.04-arm | aarch64-unknown-linux-gnu | direct-payment-timesheets_VERSION_arm64.deb; DirectPaymentTimesheets-VERSION-linux-aarch64.AppImage |
| windows-x86_64 | windows-2022 | x86_64-pc-windows-msvc | DirectPaymentTimesheets-VERSION-windows-x86_64-setup.exe |
| macos-arm64 | macos-15 | aarch64-apple-darwin | DirectPaymentTimesheets-VERSION-macos-arm64.dmg |
| macos-x86_64 | macos-15-intel | x86_64-apple-darwin | DirectPaymentTimesheets-VERSION-macos-x86_64.dmg |

## Linux and ARM32 execution

Linux builds use Rust 1.98.1 in a digest-pinned Debian 12 Bookworm container. Target
packaging uses a separately digest-pinned Bookworm container. Tool versions and
image digests are inventoried in `packaging/toolchain.json`; workflow references
are checked by packaging tests. Debian apt repositories receive security updates:
these are source-consistent builds, not a promise of bit-for-bit reproducibility.

`TARGET` is required. The mapping in `packaging/linux/common.sh` specifies Debian
and AppImage architecture and strip tool. `SOURCE_BINARY`, `STRIP` and
`OUTPUT_DIRECTORY` can override paths/tools. `SKIP_BUILD=1` requires an existing
binary plus its `.build.json` handoff metadata, validated against source, version,
target, installation kind and hash. Otherwise builds use `cargo build --release
--locked --target ...`. Output roots are isolated under
`target/packages/TARGET/{deb,appimage,tests}`. Do not reuse a deb-marked executable
for an AppImage; About/update guidance consumes this compile-time marker.

ARMv7 compilation runs on amd64, using `arm-linux-gnueabihf-gcc/g++/ar`, Rust's
ARMv7 hard-float target and `libssl-dev:armhf`. Target pkg-config paths exclude host
libraries. SQLite/compression dependencies compile with the target C toolchain.
Tests are compiled with `--no-run --message-format=json` and their exact executable
paths are captured. Both containers mount the repository at `/workspace` so those
paths and fixture references remain valid.

A pinned QEMU/binfmt image enables an ARMv7 Bookworm userspace on the x86 runner.
Inside that target container the workflow runs the compiled Rust tests, target
`dpkg-shlibdeps`, and ARMHF linuxdeploy/appimagetool. There is no architecture
inference from the host's uname. `dpkg --print-architecture` is used only to assert
that the selected packaging userspace matches the explicit target. ARM64 uses the
same scripts natively on the GitHub ARM64 runner.

ELF checks reject wrong class/machine, non-ARMv7/soft-float ARM32 and GLIBC symbol
requirements above 2.36. Debian dependencies are computed from the target binary,
with additional X11/Wayland/OpenGL/portal runtime dependencies for dynamically
loaded components. Packages include icons, desktop entries and the complete
checked-in licence/font/native notice tree.

AppImage tools are version- and SHA256-pinned in `packaging/linux/tools.json`:
linuxdeploy 1-alpha-20251107-1, appimagetool 1.9.1 and runtime 20251108. Tools are
extracted as data using Bookworm's `squashfs-tools` before execution; building
requires no FUSE device or mount. `appimage.py` checks the explicit target ELF
architecture (including ARM EABI5 hard-float), locates SquashFS from ELF section
bounds and invokes `unsquashfs`. It never executes the wrapper, including during
final package verification. This matters under QEMU: the AppImage `AI\x02`
identifier at ELF bytes 8–10 does not match the ARM binfmt rule's zero bytes,
even though the pinned ARMHF runtime and payload are the correct architecture.
Downloads retain their existing SHA256 pins and also receive ELF checks. AppDir
creation and AppImage assembly are separate, passing the pinned runtime explicitly
instead of allowing output-plugin runtime downloads. The runtime licence is also checksum-pinned and included. Copied distribution
libraries include their package copyright notices. The resulting image is extracted and
checked for executable architecture, desktop entry, notices and unresolved ELF
dependencies without starting the GUI. System graphics drivers and desktop portal
services remain host responsibilities.

ARMHF here means **ARMv7 hard-float**, not ARMv6 support for original Pi/Zero.
Choose by installed OS bitness, not just CPU capability. Bookworm is the Linux
baseline; AppImage does not remove the application's glibc requirements. QEMU
can validate CPU code/packaging, not Raspberry Pi graphics drivers or physical
hardware compatibility. Test the candidate on actual ARM32/ARM64 Raspberry Pi OS
before claiming those platforms supported.

## macOS

Each architecture builds natively and separately with Rust 1.98.1,
`DPT_INSTALLATION_KIND=macos` and `MACOSX_DEPLOYMENT_TARGET=12.0`. The bundle has
`LSMinimumSystemVersion=12.0`, stable identifier
`io.github.arnieskynet.DirectPaymentTimesheets`, Cargo-derived versions, an icns
icon generated from the existing PNG and licence resources. UI/PDF fallback fonts
are already embedded; no database, user config or payroll files enter the bundle.

Native tools validate Info.plist, single architecture, the Mach-O deployment target
and system-only library links. A final **ad-hoc** signature seals the completed
bundle; there is no Developer ID certificate, timestamp service, notarisation or
Universal binary. Ad-hoc signing satisfies executable integrity requirements but
does not establish publisher identity. `hdiutil` creates and verifies a compressed
UDZO DMG with an Applications shortcut, then mounts it read-only to inspect the
actual distributed bundle without launching it.

macOS 12.0 is the proposed minimum, pending testing on that OS. The macOS 15 runner
alone cannot establish backwards compatibility. Browser-downloaded unsigned/
unnotarised builds may need System Settings > Privacy & Security > Open Anyway
after the first attempted launch; do not disable Gatekeeper globally. Test that
flow on a real Mac before publishing installation instructions.

## Windows

The existing unsigned NSIS 3.12 per-user installer is retained. `.onInit` now
requires Windows 10 or later and 64-bit Windows, with supported-OS manifest entries
so version detection is not legacy-virtualised. Installation remains under
LOCALAPPDATA; upgrade/uninstall still preserve payroll data and use no recursive
uninstallation of user folders. The build uses the pinned MSVC Rust toolchain,
Windows SDK resources and static CRT. The installer script accepts an explicit
source binary and output directory; PE architecture, GUI subsystem and embedded
version metadata are checked before NSIS runs. The existing pinned NSIS action is
reused. Windows 10 acceptance testing is still required: the Windows Server CI
runner is not a Windows 10 compatibility test.

## Validation and next rehearsal runs

Local checks: `cargo fmt`, `cargo fmt --check`, `cargo check --locked`,
`cargo test --locked`, `git diff --check`, `bash -n` for every shell script,
`python3 -B -m unittest discover -s packaging/tests -v`, actionlint, shellcheck,
and desktop-file validation. Packaging tests cover architecture/ABI rejection,
licence staging, binary handoff isolation, all-nine aggregation, digest/source
mismatches and artifact-only workflow constraints. On an x86 Linux host with
packaging tools, a disposable C executable also exercises real Debian packaging;
it is never executed or retained as an application package.

Florence cannot run Docker/QEMU or native macOS/Windows tools in the current setup.
The following Actions runs are therefore **required proof, not claimed results**:

1. `package-rehearsal.yml`, target `linux-armv7`: prove cross-link, full tests under
   QEMU, ARMHF dependency discovery and both packages.
2. Same workflow, target `all`: prove all six architectures, nine packages and
   final source/version/checksum aggregation. Use individual target inputs to
   diagnose a failing architecture, then repeat `all` at the final reviewed SHA.
3. Install/run only with disposable data on Debian 12/13, suitable Ubuntu/Mint,
   Raspberry Pi OS ARMv7/ARM64, Windows 10/11 and both Mac architectures including
   the proposed minimum. Validate file dialogs, PDF fonts, package upgrades and
   unsigned first-launch behaviour. No production email is needed.

Do not trigger any run until the operator authorises pushing these workflow files.

## Future release phase (not enabled)

Only after successful rehearsals and approval: bump Cargo.toml once, update the
root Cargo.lock version and require tag `vVERSION` to equal the manifest version.
Schema remains separately versioned. A future tag workflow can call these same
reusable builders with one captured commit and add one publishing job after all
nine artifacts verify. Builders must retain read-only permissions; only that
future publisher would gain release-write permission. No current workflow creates,
uploads to or publishes a GitHub Release.
