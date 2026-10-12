"""Run only synthetic security/recovery unit tests; never run the application.

Continue after a failed group so native platform gaps remain visible. A group
matching no tests is an error, not a successful security result. No fixture files
are uploaded and only cargo's normal unit-test output is printed.
"""
import json
import os
from pathlib import Path
import platform
import subprocess
import sys

GROUPS = ("phase1", "backup_service::tests::", "database_recovery::",
          "payroll_archive_service::recovery::tests::")


def main():
    root = Path(__file__).resolve().parents[1]
    os.chdir(root)
    native = platform.system()
    if native not in ("Windows", "Darwin"):
        print("This runner requires native Windows or macOS; Linux checks are separate.")
        return 2
    sha = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    if sha != os.environ.get("EXPECTED_SHA"):
        print("Refusing an unreviewed revision.")
        return 2
    print(json.dumps({"sha": sha, "platform": native,
                      "machine": platform.machine()}, sort_keys=True), flush=True)
    for tool in ("rustc", "cargo"):
        subprocess.run([tool, "--version"], check=True)
    if native == "Darwin":
        subprocess.run(["xcrun", "--show-sdk-path"], check=True)
        subprocess.run(["/usr/bin/clang", "--version"], check=True)
    # The Windows-specific fixtures themselves require NTFS and fail otherwise.
    cargo = ["cargo", "+1.98.1"]
    check = subprocess.run(cargo + ["check", "--locked", "--bins", "--tests"])
    if check.returncode:
        return check.returncode
    failures = []
    native_marker = ("phase1_windows_actual_new_object_owners_match_current_token"
                     if native == "Windows" else
                     "phase1_macos_assessment_detects_comma_separated_inherited_allow")
    for group in GROUPS:
        listing = subprocess.run(cargo + ["test", "--locked", group, "--", "--list"],
                                 capture_output=True, text=True)
        count = sum(line.endswith(": test") for line in listing.stdout.splitlines())
        print(f"Security group {group}: {count} discovered tests", flush=True)
        if (listing.returncode or count == 0 or
                (group == "phase1" and native_marker not in listing.stdout)):
            print(listing.stdout)
            print(listing.stderr, file=sys.stderr)
            failures.append(group + " (discovery/native marker/compilation)")
            continue
        result = subprocess.run(cargo + ["test", "--locked", group, "--",
                                        "--test-threads=1"])
        if result.returncode:
            failures.append(group)
    print("Failed groups: " + (", ".join(failures) if failures else "none"), flush=True)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
