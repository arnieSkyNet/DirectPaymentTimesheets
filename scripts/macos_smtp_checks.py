"""Explicit, bounded native macOS CI overlay for the preserved SMTP source.

No provider interaction on Linux. Never print captured compiler/test diagnostics:
an assertion or compiler diagnostic may contain fixture secrets.
"""
import hashlib
import json
import os
from pathlib import Path
import platform
import signal
import subprocess
import sys
import tempfile

SOURCE_SHA = "d3006b7c526042a7ea5a2ab5f108a2f0d393d5dd"
REVIEWED_SHA = "77d3268a5a17b4585371a249135a0cceecfbe00b"
REVIEWED_TREE = "4748f5d329a8d6a029ee2b802240a930d256b21e"
MANIFEST_SHA256 = "8972066d653ef483a161aabb8f0ba5c5a596f6a87b65ea690437b5afff17142c"
BUILD_INPUTS = ("src", "assets", "Cargo.toml", "Cargo.lock", "build.rs",
                "packaging/windows/application.rc")
MODULE_SHA256 = "5ff10aa9d5920f88dff2a2988901e17dcfebc480d9cb51bf583c0d994e77e0bc"
TEST = "smtp_credentials::tests::macos_ci::migration_restart_missing_and_denied"
CANARIES = (b"SYNTHETIC_SECRET", b"SYNTHETIC_NATIVE_SECRET")


def inventory(root):
    result = {}
    for name in BUILD_INPUTS:
        path = root / name
        if path.is_symlink():
            raise RuntimeError("integrated_source_symlink_refused")
        paths = path.rglob("*") if path.is_dir() else [path]
        for item in paths:
            if item.is_symlink():
                raise RuntimeError("integrated_source_symlink_refused")
            if item.is_file():
                result[item.relative_to(root).as_posix()] = {
                    "sha256": hashlib.sha256(item.read_bytes()).hexdigest(),
                    "git_mode": "100755" if item.stat().st_mode & 0o111 else "100644",
                }
            elif not item.is_dir():
                raise RuntimeError("integrated_source_missing_or_special")
    return result


def load_reviewed_manifest():
    path = Path(__file__).with_name("macos_smtp_reviewed_source.json")
    if path.is_symlink():
        raise RuntimeError("reviewed_manifest_symlink_refused")
    raw = path.read_bytes()
    if hashlib.sha256(raw).hexdigest() != MANIFEST_SHA256:
        raise RuntimeError("reviewed_manifest_hash_mismatch")
    data = json.loads(raw)
    if (data.get("schema") != "dpts-reviewed-build-inputs/v1"
            or data.get("reviewed_commit") != REVIEWED_SHA
            or data.get("reviewed_tree") != REVIEWED_TREE
            or data.get("acquired_commit") != SOURCE_SHA
            or data.get("roots") != list(BUILD_INPUTS)):
        raise RuntimeError("reviewed_manifest_identity_mismatch")
    return data["files"]


def verify_reviewed_source(root):
    # Compare to the independently preserved original, not two matching clones.
    if inventory(root) != load_reviewed_manifest():
        raise RuntimeError("build_inputs_differ_from_original_reviewed_source")


def verify_integration(source, integrated):
    """Do not claim the integrated branch is tested if any build input differs."""

    if inventory(source) != inventory(integrated):
        raise RuntimeError("integrated_build_inputs_differ_from_reviewed_source")


def overlay(source, fixture):
    module = source / "src/smtp_credentials.rs"
    original = module.read_bytes()
    if hashlib.sha256(original).hexdigest() != MODULE_SHA256:
        raise RuntimeError("source_module_hash_mismatch")
    if not original.rstrip().endswith(b"}"):
        raise RuntimeError("test_module_boundary_missing")
    # The exact pinned file ends with its tests module. Production code unchanged.
    module.write_bytes(original.rstrip()[:-1] + b'\ninclude!("macos_smtp_native_fixture.rs");\n}\n')
    (source / "src/macos_smtp_native_fixture.rs").write_bytes(fixture.read_bytes())
    return original


def run(args, source, env, timeout, logs):
    with tempfile.TemporaryFile() as output:
        process = subprocess.Popen(args, cwd=source, env=env, stdout=output,
                                   stderr=subprocess.STDOUT, start_new_session=True)
        try:
            result = process.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait(timeout=10)
            raise RuntimeError("operation_timeout_process_group_killed_keychain_cleanup_unknown") from None
        output.seek(0)
        data = output.read()
    logs.append(data)
    if any(secret in data for secret in CANARIES):
        raise RuntimeError("captured_output_plaintext_detected_no_upload")
    if result:
        raise RuntimeError("operation_failed_diagnostics_withheld")
    return data


def main():
    if (sys.platform != "darwin" or os.environ.get("GITHUB_ACTIONS") != "true"
            or os.environ.get("RUNNER_ENVIRONMENT") != "github-hosted"
            or platform.machine() != os.environ.get("DPT_EXPECTED_ARCH")):
        raise RuntimeError("requires_expected_github_hosted_macos_architecture")
    source = Path(sys.argv[1]).resolve()
    fixture = Path(__file__).with_name("macos_smtp_native_fixture.rs")
    env = {k: v for k, v in os.environ.items() if k in (
        "HOME", "PATH", "TMPDIR", "SDKROOT", "DEVELOPER_DIR", "RUSTUP_HOME", "CARGO_HOME")}
    env.update(RUSTUP_TOOLCHAIN="1.98.1", CARGO_INCREMENTAL="0", CARGO_BUILD_JOBS="2",
               CARGO_PROFILE_TEST_DEBUG="0", DPT_MACOS_SYNTHETIC_CI="1")
    logs = []
    head = run(["git", "rev-parse", "HEAD"], source, env, 10, logs).decode().strip()
    if head != SOURCE_SHA:
        raise RuntimeError("source_commit_mismatch")
    if run(["git", "status", "--porcelain=v1", "--untracked-files=all"], source, env, 10, logs).strip():
        raise RuntimeError("source_checkout_not_clean")
    integrated = Path(__file__).resolve().parents[1]
    verify_reviewed_source(source)
    verify_reviewed_source(integrated)
    verify_integration(source, integrated)
    original = overlay(source, fixture)
    try:
        build = run(["cargo", "test", "--locked", "--bin", "direct_payment_timesheets",
                     "--no-run", "--message-format=json"], source, env, 2400, logs)
        artifacts = [r for line in build.splitlines() if line.startswith(b"{")
                     for r in [json.loads(line)] if r.get("reason") == "compiler-artifact"
                     and r.get("target", {}).get("name") == "direct_payment_timesheets"
                     and r.get("profile", {}).get("test") and r.get("executable")]
        if len(artifacts) != 1:
            raise RuntimeError("test_binary_inventory_mismatch")
        binary = Path(artifacts[0]["executable"]).resolve()
        if not binary.is_relative_to(source / "target") or not binary.is_file():
            raise RuntimeError("unexpected_test_binary")
        inventory = run([str(binary), "--list", "--ignored"], source, env, 20, logs)
        if inventory.splitlines().count((TEST + ": test").encode()) != 1:
            raise RuntimeError("native_test_missing")
        result = run([str(binary), "--exact", TEST, "--ignored", "--test-threads=1"],
                     source, env, 180, logs)
        if b"1 passed; 0 failed; 0 ignored;" not in result:
            raise RuntimeError("native_test_completion_unconfirmed")
        print("PASS: real Keychain migration/retrieval/fresh-process persistence/missing-entry/cleanup; "
              "mocked denial; generated config and backup audit; captured-output audit. No artifacts uploaded.")
    finally:
        (source / "src/smtp_credentials.rs").write_bytes(original)
        (source / "src/macos_smtp_native_fixture.rs").unlink(missing_ok=True)


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        # Fixed errors only; never stringify arbitrary provider/build exceptions.
        known = str(error) if type(error) is RuntimeError else "unexpected_failure_diagnostics_withheld"
        print("STOP: " + known)
        sys.exit(1)
