# Release builds and artifact-only packaging rehearsals

[v1.0.3](https://github.com/arnieSkyNet/DirectPaymentTimesheets/releases/tag/v1.0.3)
was published on 25 September 2026 from commit
`7415778d4cde1482b70102289e5ece2a33f2a235`. Its ten downloadable packages comprise
six Linux packages, two Windows installers (x86-64 and ARM64), and two macOS DMGs.
The application schema is 32. Package versions come from Cargo.toml; references
to 1.0.2 below describe earlier rehearsals and hardware tests.

The packaging workflows remain artifact-only: they build and validate candidates,
but do not publish GitHub releases. Publication is a separate authorised action.

## Entry point and source identity

For a future candidate, after changes are reviewed, committed and pushed
**by an authorised operator**,
run `.github/workflows/package-rehearsal.yml` (Multi-platform package rehearsal).
Its `target` input accepts `all`, `linux-x86_64`, `linux-armv7`, `linux-aarch64`,
`windows-x86_64`, `windows-arm64`, `macos-arm64` or `macos-x86_64`. Start with `linux-armv7` to prove
the highest-risk path, then run `all` from the same reviewed revision.

The preparation job captures checkout HEAD and Cargo's package version. Every
reusable workflow checks out that exact SHA with persisted credentials disabled.
All workflows use `contents: read`, only manual/reusable triggers, and have **no
release publisher or tag/push trigger**. No GitHub Release can be created by these
jobs. The three platform workflows also accept independent manual dispatches for
diagnosis, but only the `all` orchestrator verifies a complete ten-package set.

Each package carries adjacent `.json` provenance and `.sha256` files. They record
source SHA, version, target, installation kind, tool pins and package digest.
An all-target run checks the exact package inventory, rejects duplicate/missing
packages, mixed revisions/versions and digest mismatches, flattens the packages,
and writes `rehearsal-manifest.json` and `SHA256SUMS`. Actions artifacts are retained
for 30 days. These workflows do not upload release assets. A failed platform prevents
the complete artifact; successful independent jobs can still be inspected.

## Targets

| Workflow target | Runner | Rust target | Packages (VERSION from Cargo) |
|---|---|---|---|
| linux-x86_64 | ubuntu-24.04 | x86_64-unknown-linux-gnu | direct-payment-timesheets_VERSION_amd64.deb; DirectPaymentTimesheets-VERSION-linux-x86_64.AppImage |
| linux-armv7 | ubuntu-24.04, cross compiler + QEMU | armv7-unknown-linux-gnueabihf | direct-payment-timesheets_VERSION_armhf.deb; DirectPaymentTimesheets-VERSION-linux-armhf.AppImage |
| linux-aarch64 | ubuntu-24.04-arm | aarch64-unknown-linux-gnu | direct-payment-timesheets_VERSION_arm64.deb; DirectPaymentTimesheets-VERSION-linux-aarch64.AppImage |
| windows-x86_64 | windows-2022 | x86_64-pc-windows-msvc | DirectPaymentTimesheets-VERSION-windows-x86_64-setup.exe |
| windows-arm64 | windows-2022, MSVC cross compiler | aarch64-pc-windows-msvc | DirectPaymentTimesheets-VERSION-windows-arm64-setup.exe |
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
flow on real Macs, including the minimum OS, before declaring release acceptance.
The [README installation guide](../README.md#macos) describes the intended GUI
procedure; its instructions do not imply that hardware acceptance is complete.

## Windows

The unsigned NSIS 3.12 per-user installer now has separate x86-64 and ARM64
payloads. `windows-arm64` uses `aarch64-pc-windows-msvc` (not ARM64EC or an
emulated x64 application), cross-compiled on windows-2022 with the ARM64 Visual
Studio C++ component and pinned Rust 1.98.1. ARM64 application and tests are checked with
`cargo check --locked --target aarch64-pc-windows-msvc --bins --tests`, without
linking test executables; x86-64 tests still execute. The ARM64 release build
still links the distributable executable. Both build with static CRT and Windows SDK
version/icon resources. Rust's [MSVC target documentation](https://doc.rust-lang.org/rustc/platform-support/windows-msvc.html)
specifies Windows 10 or later and supports architectural cross-compilation.

The builder verifies the binary handoff `.build.json` against checkout SHA,
version, target, installation kind and SHA256 before packaging. It then checks
PE machine (0x8664 for x64, 0xAA64 for ARM64), PE32+, GUI subsystem and embedded
version resources. This validates the application payload, not the standard x86
NSIS bootstrap, which runs under Windows ARM's x86 emulation. Artifact names,
output directories and provenance targets remain architecture-specific.

NSIS uses `AtLeastWin10` and `IsNativeARM64` from x64.nsh. The ARM64 installer
rejects non-ARM64 systems; the x64 installer directs ARM64 users to the native
installer before writing any files. The supported-OS manifest remains enabled.
Both use the same fixed LOCALAPPDATA program directory, shortcuts and 64-bit HKCU
uninstall key: upgrades replace the application, including an existing emulated
x64 installation on ARM64, without creating a parallel install. No payroll-data
paths are installed or removed. Uninstall retains its exact file list and only
removes empty directories; never recursive user-folder deletion. Close the app
before upgrading. No downgrade prevention or automatic update installation is added.

CI must verify cross-linking, resource inspection, pinned NSIS compilation and
both architecture branches. On actual Windows 10 ARM64 and Windows 11 ARM64,
build and run the tests natively and check fresh install, GUI startup/file dialogs/PDFs,
upgrade from an older per-user install (including x64 on ARM where applicable),
locked executable behaviour, and uninstall preserving disposable payroll data.
Check wrong-architecture rejection on x64 hardware. No ARM64 runtime compatibility
claim follows from compiling on an x64 Windows Server runner.
The earlier 1.0.2 x64 candidate was tested on Windows 10; that does not validate
the published 1.0.3 ARM64 or rebuilt x64 packages.

## Validation and future rehearsal runs

Local checks: `cargo fmt`, `cargo fmt --check`, `cargo check --locked`,
`cargo test --locked`, `git diff --check`, `bash -n` for every shell script,
`python3 -B -m unittest discover -s packaging/tests -v`, actionlint, shellcheck,
and desktop-file validation. Packaging tests cover architecture/ABI rejection,
licence staging, binary handoff isolation, all-ten aggregation, digest/source
mismatches and artifact-only workflow constraints. On an x86 Linux host with
packaging tools, a disposable C executable also exercises real Debian packaging;
it is never executed or retained as an application package.

Florence cannot run Docker/QEMU or native macOS/Windows tools in the current setup.
Full rehearsal 35541897015 at source
`15bd2620a45459bce3330453c4eb8a3b8bba1eb9` produced all nine 1.0.2 packages.
Those 1.0.2 Windows 10 and ARMHF packages received real-hardware testing. A later
startup-sizing change superseded those binaries. These are historical rehearsal
results, not the current download inventory: v1.0.3 is now published with all ten
packages. Publication alone does not establish completion of the remaining
hardware and minimum-OS checks documented here.

For future releases, the reviewed revision should complete these checks:

1. `package-rehearsal.yml`, target `linux-armv7`: prove cross-link, full tests under
   QEMU, ARMHF dependency discovery and both packages.
2. Same workflow, target `all`: prove all seven platform/architecture combinations, ten packages and
   final source/version/checksum aggregation. Use individual target inputs to
   diagnose a failing architecture, then repeat `all` at the final reviewed SHA.
3. Install/run only with disposable data on Debian 12/13, suitable Ubuntu/Mint,
   Raspberry Pi OS ARMv7/ARM64, Windows 10/11 and both Mac architectures including
   the proposed minimum. Validate file dialogs, PDF fonts, package upgrades and
   unsigned first-launch behaviour. No production email is needed.

Future workflow runs still require operator authorisation.

## Publication and future automation

The published v1.0.3 release corresponds to Cargo version 1.0.3 at the commit
above. Schema versioning remains separate. For future releases, require tag
`vVERSION` to match the tested manifest version and publish the verified package
set from that same revision.

Automated release publication is not enabled. A future tag workflow could call
these reusable builders with one captured commit and add a publishing job after
all ten artifacts verify. Builders must retain read-only permissions; only an
explicitly authorised publisher would gain release-write permission. No current
workflow creates, uploads to or publishes a GitHub Release.

### ARM Linux runtime validation

Packaged `arm` and `aarch64` executables (`deb`/`appimage` installation markers)
default `LIBGL_ALWAYS_SOFTWARE` to `1` at the beginning of `main`, before threads
or graphics initialisation. An explicit environment value is preserved. This
covers desktop, AppImage and direct executable launches without machine-specific
launcher edits. Source builds, x86-64 Linux, Windows and macOS keep their defaults.
The policy also applies to ARM64 packages, but ARM64 hardware validation remains
outstanding; it is not evidence that all ARM GPUs require software rendering.

On the tested ARMv7 Pi 3B+ / Bookworm / X11 / LXDE-pi, both 1.0.2 package
formats render correctly with the automatic software default, without manual
environment changes. The separate initial-window problem was observed as an
actual 1x1 X11 window despite a 1000x800 size hint. The startup-sizing guard now
rejects unusable monitor geometry and preserves a sensible default, without
forcing maximisation. Final rebuilt ARMHF packages still need a normal launch
confirming visible window dimensions without manual Maximise. No machine-specific
LXDE menu configuration is part of the package.

## Public downloads and future publication checks

The README links to all ten actual v1.0.3 release assets using fixed
`/releases/download/v1.0.3/` URLs, including
`DirectPaymentTimesheets-1.0.3-windows-arm64-setup.exe`. Asset names and URLs were
checked against the published GitHub release. The actual macOS bundle is
`Direct Payments Timesheets.app` (with spaces), not `DirectPaymentTimesheets.app`.
Its minimum deployment target is 12.0 and its signature is ad-hoc, not Developer
ID/notarised.

For future publication, verify the ten-package inventory, same-source provenance
and checksums, confirm each asset name matches the README, and record the status
of ARM32 window behaviour, Mac first-launch/minimum-OS and ARM64 hardware checks.
Documentation edits do not authorise a tag, release change, upload or workflow run.
