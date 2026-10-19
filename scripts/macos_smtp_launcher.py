"""Prepare only an ephemeral d300 test harness from a verified main controller.

No native test or credential API is invoked here. No runtime bypass options.
"""
import hashlib
import importlib.util
import os
from pathlib import Path
import re
import subprocess
import sys

SOURCE_SHA = "d3006b7c526042a7ea5a2ab5f108a2f0d393d5dd"
OVERLAY_FILES = ("macos_smtp_checks.py", "macos_smtp_reviewed_source.json", "test_macos_smtp_checks.py")
# Native fixture remains the exact file from the pinned application commit.
FIXTURE_SHA256 = "907a05c66bc6fd1e55d433d5e98ccebfb78f450a3658c75863e19ec6b015f590"


def dispatch_sha():
    sha = os.environ.get("GITHUB_SHA", "")
    if (os.environ.get("GITHUB_EVENT_NAME") != "workflow_dispatch"
            or os.environ.get("GITHUB_REF") != "refs/heads/main"
            or not re.fullmatch(r"[0-9a-f]{40}", sha)):
        raise RuntimeError("requires_manual_main_dispatch")
    return sha


def checkout_identity(root, expected):
    for args, failure in ((["rev-parse", "HEAD"], "checkout_sha_mismatch"),
                          (["status", "--porcelain=v1", "--untracked-files=all"], "checkout_not_clean")):
        try:
            result = subprocess.run(["git", "-C", str(root), *args], capture_output=True,
                                    timeout=10, check=False)
        except (OSError, subprocess.TimeoutExpired):
            raise RuntimeError("checkout_identity_unconfirmed") from None
        if result.returncode or result.stderr:
            raise RuntimeError("checkout_identity_unconfirmed")
        if args[0] == "rev-parse":
            if result.stdout.decode().strip() != expected:
                raise RuntimeError(failure)
        elif result.stdout.strip():
            raise RuntimeError(failure)


def checks_module(controller):
    path = controller / "scripts/macos_smtp_checks.py"
    regular_file(path)
    spec = importlib.util.spec_from_file_location("verified_macos_smtp_checks", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    if module.SOURCE_SHA != SOURCE_SHA:
        raise RuntimeError("controller_source_pin_mismatch")
    return module


def regular_file(path):
    if path.is_symlink() or not path.is_file():
        raise RuntimeError("overlay_file_missing_or_symlink")
    return path.read_bytes()


def prepare(controller, harness, source):
    expected = dispatch_sha()
    roots = (controller, harness, source)
    if any(root.is_symlink() for root in roots):
        raise RuntimeError("checkout_root_symlink_refused")
    controller, harness, source = (root.resolve() for root in roots)
    if len({controller, harness, source}) != 3 or any(
            a in b.parents for a in (controller, harness, source)
            for b in (controller, harness, source) if a != b):
        raise RuntimeError("checkout_roots_not_disjoint")
    for root, sha in ((controller, expected), (harness, SOURCE_SHA), (source, SOURCE_SHA)):
        checkout_identity(root, sha)
        if (root / "scripts").is_symlink() or not (root / "scripts").is_dir():
            raise RuntimeError("scripts_directory_refused")
    checks = checks_module(controller)
    manifest = checks.load_reviewed_manifest()
    if len(manifest) != 111:
        raise RuntimeError("reviewed_input_count_mismatch")
    # Main is the controller, never an asserted equivalent application checkout.
    checks.verify_reviewed_source(harness)
    checks.verify_reviewed_source(source)
    checks.verify_integration(source, harness)
    fixture = harness / "scripts/macos_smtp_native_fixture.rs"
    original_fixture = regular_file(fixture)
    if hashlib.sha256(original_fixture).hexdigest() != FIXTURE_SHA256:
        raise RuntimeError("native_fixture_hash_mismatch")
    payload = {}
    for name in OVERLAY_FILES:
        src, dst = controller / "scripts" / name, harness / "scripts" / name
        data = regular_file(src)
        if dst.is_symlink() or (dst.exists() and not dst.is_file()):
            raise RuntimeError("overlay_destination_refused")
        payload[name] = data
    # All evidence is checked before the first write. Only these three test
    # support files are changed; source and application inputs remain pristine.
    for name, data in payload.items():
        dst = harness / "scripts" / name
        dst.write_bytes(data)
        if regular_file(dst) != data:
            raise RuntimeError("overlay_copy_unconfirmed")
    if regular_file(fixture) != original_fixture:
        raise RuntimeError("native_fixture_changed")
    checks.verify_reviewed_source(harness)
    checks.verify_reviewed_source(source)
    checks.verify_integration(source, harness)
    checkout_identity(source, SOURCE_SHA)
    print("PASS: main controller verified; two immutable d300 checkouts; 111 original hashes/modes verified; three test-support files copied; native fixture unchanged")


if __name__ == "__main__":
    try:
        if len(sys.argv) != 4:
            raise RuntimeError("requires_controller_harness_source_paths")
        prepare(*(Path(value) for value in sys.argv[1:]))
    except Exception as error:
        message = str(error) if type(error) is RuntimeError else "launcher_failure_diagnostics_withheld"
        print("STOP: " + message)
        sys.exit(1)
