# Proposed iMolly hostname correction and benign desktop identity probe

Uncommitted candidates only. No probe, fixtures, supervisor controls or desktop
child has been executed by this preparation. Published S/W/L and the earlier
safe-preflight candidate remain unchanged. Production code is outside this delta.

## Candidate boundaries

Source candidate: based on S 0df2019b45a77995b869685621ea8420501f7956.
Only two Rust hostname literals change from Molly to IMOLLY. The matching fixture
documentation changes too. GetComputerNameW, native AMD64/no-WOW64 admission,
synthetic-risk controls and the eight-case contract remain byte-for-byte intact.
Corrected fixture SHA-256:
f457d426fb87edfffa62afc6d79be854448380e9955ffddc728dd533486b9727

Controller candidate: based on published main L
2d6c5478a34c1086a1ecae671f50e046f09f7d40, containing W as its parent.
The local Python gate requires DPT_NATIVE_SQLITE_HOST=IMOLLY and independently
platform.node().casefold()==imolly. Its reviewed fixture hash matches the source
candidate; tests exercise accepted case variants of the OS name, exact environment
values, spoofed values, rejected Molly/unrelated hosts and unsupported platforms.
The historical profile/risk identifiers remain unchanged; they do not admit an
extra hostname. The WindowsSecurity non-elevated-token refusal is unchanged.

No Rust fixture is copied into main/controller. Existing native/Phase 1 workflows
and all four published S/W pins are unchanged. They still point at the OLD S/W,
so they must not be used to claim execution of these candidates. New source and
controller commits, fixture verification and separately reviewed launcher pins
are required before any future native run. The previously reviewed safe-preflight
candidate also has old immutable pins/fingerprints and must be re-reviewed before
being used with corrected source. No new final commit IDs are invented here.

## Exact benign operator command: preparation only

Read scripts/windows_medium_identity_probe.ps1 first. The exact single-line
operator command is scripts/windows_medium_identity_probe.command.txt, SHA-256:
694e2eff33a8b2891431485925434b5fd095b4396052bd759401dbe42675381b
The PowerShell source SHA-256 is:
610a3f64f4c25ff972e97ad63377ffa7816a2eb1a5ce1994d506d75a5be605b1

The encoded payload is exactly the source script wrapped in a scriptblock with
-ApprovedDesktopIdentityProbe. Static tests decode and compare it byte-for-byte;
no probe or PowerShell execution is needed for those tests. The command must be
separately approved by its exact bytes/digest and pasted by the operator into a
normal existing desktop command prompt, not run through SSH. No file placement on
Molly is required for that inline command. No execution-policy bypass is used.
The .ps1 entry point itself refuses invocation without its approval switch.

The probe refuses SSH/noninteractive-service contexts. It queries only its own
current identity token using already loaded .NET token-query APIs, without
Add-Type, token duplication, impersonation, another-process access, elevation,
service/broker changes, a child process, filesystem writes or supervisor controls.
It emits one JSON object with ONLY account, integrity_level and elevated. No SID,
paths, hostname, PID, environment, exception or other diagnostics are exported.
Exit 0 requires account IMOLLY\mark, exactly MEDIUM (not MEDIUM_PLUS), elevated=false
and resource disposal without an observed exception. Other results/failures exit 2. Disabled/remote
invocation refuses without a report. API/query failures can produce null fields;
missing fields/results are not success. Reflection compatibility and PowerShell
syntax/native execution remain unverified here: no local PowerShell is installed.

Prior read-only observation: SSH is session 0, high IL, elevated, elevation type
Default (no linked limited token). Existing Explorer PID 9832 in session 1 was
same-user, Limited, MEDIUM and non-elevated. Those observations do not prove a
future child launch or its token. The probe must independently attest its own
process. An SSH-to-desktop broker is not implemented or authorized.

## Python provisioning proposal (not implemented)

Scope: a private native AMD64 CPython 3.12 runtime for the synthetic controller,
stdlib only, with absolute executable paths and no global PATH/registry changes.
Do not invoke Windows Store aliases or fetch/install anything under this approval.

Current upstream nuance: Python 3.12.10 was the last full regular release with
Windows binary installers; later 3.12 security releases are source-only. See
https://www.python.org/downloads/release/python-31213/ and
https://peps.python.org/pep-0693/ . The October 2026 release announcement lists
3.12.15: https://blog.python.org/2026/10/python-31022-31117/ .

Preferred security proposal: separately approve acquisition/verification of the
current official 3.12 security source and a native AMD64 build on an isolated
Windows builder, with pinned dependency provenance, build/toolchain receipt and
SHA-256 for the complete resulting runtime. No such build/download is performed
now. Review a complete deployable runtime (stdlib, ctypes, native dependencies),
not an unverified interpreter executable alone. Package it privately for a later
approved application-local deployment, e.g.
C:\Users\Wizdom\AppData\Local\DirectPaymentTimesheets-synthetic-tools\Python312.
Validate source provenance, PE AMD64, version, complete runtime hashes and native
IsWow64Process2 in a separately approved readiness step. No pip packages or live
application inputs are required by the controller.

Alternative: explicitly approve the older official AMD64 3.12.10 binary after
reviewing its patch-level/security limitations. Verify the exact upstream digest
and PSF signing provenance before any deployment. This is not assumed equivalent
to current security-patched 3.12 and is not silently selected by this plan. A newer
Python minor would instead require a separate controller compatibility decision.
Upstream installation options: https://docs.python.org/3.12/using/windows.html .

## Dedicated storage proposal (not implemented)

Proposed fresh parent:
C:\Users\Wizdom\AppData\Local\DirectPaymentTimesheets-synthetic-tests
This path was absent in the prior passive checks; recheck before any creation.
No directory or ACL is created/repaired by this preparation. No existing generic
Temp directory or retained evidence root is relabelled synthetic-only.

If separately approved, create the previously absent parent with its restrictive
security descriptor AT creation: owner is the verified IMOLLY\mark SID, protected
non-null DACL, inheritable FullControl only for that SID and SYSTEM. Refuse existing
or unfamiliar objects; never adopt or repair them. Read back ownership, descriptor
protection and every ACE; refuse unexpected trustees/types, reparse components,
non-directories or failed native architecture/token admission. Require a genuine
medium/non-elevated controller context. Existing ancestry must be inspected and
namespace handles retained according to the reviewed Windows fixture policy.
This does not protect against administrators or other processes of the same user.

Any future per-suite random root remains created by the reviewed fixture-storage
helper, under explicitly selected TEMP/TMP after verifying the approved parent.
Check child and pre/post recovery files against the existing owner/DACL and
identity rules. No directory-writing proof, handle guard or permission test is
claimed from the current passive observations. Raw synthetic recovery evidence
and sanitized >=90-day receipts have separate retention/approval requirements.

## Future compile-only environment (not activated)

The existing VS 2022 x64 wrapper can be used inside a separately approved
non-elevated process-local command environment:

    call "C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat" 10.0.26100.0 -vcvars_ver=14.44

Reverify selected compiler/linker, SDK and Rust 1.98.1; use an isolated immutable
source checkout and offline/locked --no-run build. No permanent PATH, account,
UAC, SSH service or OS setting change is proposed. Activation/build execution is
not authorized by this document. Ordinary build timeouts do not establish native
supervisor containment or compiler descendant reaping.

## Separate decisions required

1. Review these exact isolated source/controller trees, hashes and file allowlists;
   separately authorize any commit/publication and subsequent literal launcher pins.
2. Approve only the exact operator command/digest for one benign desktop identity
   probe. No fixtures, supervisor controls, automatic retry or remote launch.
3. Choose the Python provenance/patch policy, then review an exact package and
   deployment plan before authorizing any acquisition/build/provisioning.
4. Approve the exact storage path and creation-only security descriptor in a later
   step, without modifying existing parents or permissions.
5. After these prerequisites, separately approve compile-only readiness. Native
   execution, reporting risk and public sanitized receipts remain later decisions.
