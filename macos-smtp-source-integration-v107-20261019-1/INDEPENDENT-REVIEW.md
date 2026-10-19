# Independent source integration review — 19 October 2026

Result: PASS for the local source integration and workflow source-selection design.
This is not macOS runtime acceptance or approval to publish/execute workflows.

The reviewer inspected the repository history, actual workspace files, prepared
workflow, native fixture, harness and its regression tests independently of the
integration author's equivalence receipt.

- Current HEAD `e5811307706d62f8a1613e7ae84e15d0b7b3ed75` and reviewed bundle
  commit `77d3268a5a17b4585371a249135a0cceecfbe00b` have the same parent,
  `5abca6816b1a74aa6f41304a51fd5e4bb2128b82`. They are sibling developments.
  The current HEAD changes only setup documentation/images; it does not contain
  the credential implementation. This explains the discrepancy without suggesting
  that the tested encrypted implementation was removed from this branch.
- A direct, independent comparison of all 31 paths changed by reviewed commit
  `77d3268` against `git show` blobs found **zero mismatches** in the integrated
  workspace. All 72 paths added/changed by the setup-guide commit remain exactly
  equal to their current HEAD blobs.
- The complete reviewed delta preserves the Windows guarded publication,
  development isolation, credential abstraction, config migration and backup
  integration together. The Windows publication module and Windows tests remain
  byte-for-byte equal to the reviewed source. There is one credential abstraction,
  not a second macOS production implementation. Selective transplantation of only
  the SMTP module would have risked omitting publication dependencies.
- The preserved Debian report pins this exact reviewed commit and records 78
  ordinary unit tests plus 24 separately executed real Secret Service probes.
  The reviewed commit records Windows 961 passed, zero failed, five ignored and
  separate native migration/restart success. These remain prior platform evidence;
  this review did not rerun or manufacture Windows/Debian native results.
- The workflow checks out the reviewed immutable commit separately from the
  dispatch checkout. The harness checks the exact source commit, clean source
  checkout, credential-module hash and equality of integrated source/build inputs
  before applying its test-only overlay. Extra source modules and changed lock
  files fail closed. Actual build inputs inspected in `build.rs` are covered by
  the selected source/assets/Cargo/build/resource roots.
- The fixture calls the real macOS backend using a synthetic test service and
  UUID accounts. It distinguishes native migration/retrieval/fresh-process and
  missing-entry checks from simulated provider denial. Ordinary unit tests use
  the memory provider and cannot be counted as Keychain integration acceptance.
- Independently executed the eight Python harness tests: **8 passed, zero failed**.
  They access no Keychain. The review identified optional root-directory-symlink
  hardening for the inventory guard; the author will implement and test it.

Remaining limits: no macOS native compilation or execution was performed here.
Apple SDK/C helper compilation, hosted runner Keychain availability and both
architectures must actually pass on GitHub. Headless runners do not establish
GUI prompts, real locked/ACL-denied Keychains, signing/update behavior or
login/reboot persistence. No release acceptance follows from local preparation.
The author separately reports successful exact-commit HTTPS retrieval from GitHub;
this reviewer did not repeat the remote request. No commits, pushes, workflow
triggers, VM actions or production credential access were performed by this review.

## Final focused review addendum

The root-path symlink guard is now implemented before directory traversal, and
its regression fixture specifically replaces the assets root with a directory
symlink and requires refusal. This resolves the optional hardening observation.
The latest retained harness log reports eight passing tests.

Reviewed retained focused-result JSON and logs: configuration 15 passed,
backup service eight passed and email settings nine passed, with zero failures.
The author additionally reports an authorised unsandboxed SMTP group rerun with
eight passed, zero failed, one native test ignored. The earlier retained
`smtp-regression.log` records the sandbox's loopback-bind denial (seven passed,
one failed, one ignored); this is not a code-regression acceptance result. At
addendum inspection the successful rerun was not yet in that log or the focused
JSON. The author was asked to preserve the successful tool-result receipt so
both attempts remain explicit. No native or unit groups were repeated by this
reviewer for the addendum.

No source-integration blocker was found. A retained successful SMTP rerun receipt
is an evidence-reporting requirement, not permission to infer a native platform
pass. macOS native compilation/Keychain execution remains outstanding.

### SMTP evidence receipt resolved

Inspected final `RESULTS.json` and `REPORT.md`. The successful SMTP rerun now has
an explicit transcribed actual-tool-result receipt: exec chunk `2a4019`, exit 0,
exact bounded offline command, and `8 passed; 0 failed; 1 ignored`. The original
sandbox failure remains preserved and separately explained. The receipt clearly
states that no native provider executed. This sufficiently resolves the reporting
gap identified above; this reviewer did not repeat the run or claim direct
independent observation of the original tool chunk. Final review remains PASS
for local integration/preparation, with native macOS execution still pending.
