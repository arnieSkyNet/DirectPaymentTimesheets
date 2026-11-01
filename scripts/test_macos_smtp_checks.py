"""Harness tests use fake commands/files only; these are NOT Keychain tests."""
import importlib.util
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("macos_smtp_checks", Path(__file__).with_name("macos_smtp_checks.py"))
checks = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checks)


class HarnessTests(unittest.TestCase):
    def test_published_manifest_identity_and_hash(self):
        files = checks.load_reviewed_manifest()
        self.assertEqual(len(files), 111)
        self.assertEqual(files["src/smtp_credentials.rs"]["sha256"], checks.MODULE_SHA256)
        self.assertEqual(checks.SOURCE_SHA, "d3006b7c526042a7ea5a2ab5f108a2f0d393d5dd")
        self.assertEqual(checks.REVIEWED_SHA, "77d3268a5a17b4585371a249135a0cceecfbe00b")

    def test_manifest_tampering_and_wrong_provenance_are_refused(self):
        with patch.object(checks.Path, "read_bytes", return_value=b"tampered"):
            with self.assertRaisesRegex(RuntimeError, "manifest_hash_mismatch"):
                checks.load_reviewed_manifest()
        original = json.loads(Path(__file__).with_name("macos_smtp_reviewed_source.json").read_bytes())
        for key, value in (("reviewed_commit", "0" * 40), ("reviewed_tree", "0" * 40),
                           ("acquired_commit", "0" * 40), ("roots", ["src"]),
                           ("schema", "unknown")):
            changed = dict(original, **{key: value})
            raw = json.dumps(changed).encode()
            with self.subTest(key=key), patch.object(checks.Path, "read_bytes", return_value=raw), \
                    patch.object(checks, "MANIFEST_SHA256", checks.hashlib.sha256(raw).hexdigest()):
                with self.assertRaisesRegex(RuntimeError, "manifest_identity_mismatch"):
                    checks.load_reviewed_manifest()

    def test_matching_source_and_harness_drift_cannot_replace_original(self):
        with tempfile.TemporaryDirectory() as tmp:
            source, integrated = Path(tmp) / "source", Path(tmp) / "integrated"
            for root in (source, integrated):
                for directory in ("src", "assets", "packaging/windows"):
                    (root / directory).mkdir(parents=True)
                for name in ("Cargo.toml", "Cargo.lock", "build.rs", "src/main.rs", "packaging/windows/application.rc"):
                    (root / name).write_bytes(b"reviewed")
            expected = checks.inventory(source)
            with patch.object(checks, "load_reviewed_manifest", return_value=expected):
                checks.verify_reviewed_source(source)
                checks.verify_reviewed_source(integrated)
                for root in (source, integrated):
                    (root / "src/main.rs").write_bytes(b"same unreviewed change")
                checks.verify_integration(source, integrated)
                for root in (source, integrated):
                    with self.assertRaisesRegex(RuntimeError, "differ_from_original"):
                        checks.verify_reviewed_source(root)

    def test_original_manifest_refuses_missing_extra_and_executable_changes(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for directory in ("src", "assets", "packaging/windows"):
                (root / directory).mkdir(parents=True)
            for name in ("Cargo.toml", "Cargo.lock", "build.rs", "src/main.rs", "packaging/windows/application.rc"):
                (root / name).write_bytes(b"reviewed")
            expected = checks.inventory(root)
            with patch.object(checks, "load_reviewed_manifest", return_value=expected):
                extra = root / "src/extra.rs"
                extra.write_bytes(b"extra")
                with self.assertRaisesRegex(RuntimeError, "differ_from_original"):
                    checks.verify_reviewed_source(root)
                extra.unlink()
                module = root / "src/main.rs"
                module.chmod(0o700)
                with self.assertRaisesRegex(RuntimeError, "differ_from_original"):
                    checks.verify_reviewed_source(root)
                module.unlink()
                with self.assertRaisesRegex(RuntimeError, "differ_from_original"):
                    checks.verify_reviewed_source(root)

    def test_wrong_checkout_sha_and_dirty_checkout_stop_before_overlay(self):
        env = {"GITHUB_ACTIONS": "true", "RUNNER_ENVIRONMENT": "github-hosted", "DPT_EXPECTED_ARCH": "arm64"}
        for outputs, error in (([b"77d3268a5a17b4585371a249135a0cceecfbe00b\n"], "source_commit_mismatch"),
                               ([(checks.SOURCE_SHA + "\n").encode(), b" M src/main.rs\n"], "source_checkout_not_clean")):
            with self.subTest(error=error), patch.dict(os.environ, env), \
                    patch.object(checks.sys, "platform", "darwin"), \
                    patch.object(checks.platform, "machine", return_value="arm64"), \
                    patch.object(checks.sys, "argv", ["checks", "/unused"]), \
                    patch.object(checks, "run", side_effect=outputs), \
                    patch.object(checks, "overlay") as overlay:
                with self.assertRaisesRegex(RuntimeError, error):
                    checks.main()
                overlay.assert_not_called()

    def test_integration_requires_identical_build_inputs_and_no_extra_module(self):
        with tempfile.TemporaryDirectory() as tmp:
            source, integrated = Path(tmp) / "source", Path(tmp) / "integrated"
            for root in (source, integrated):
                for directory in ("src", "assets", "packaging/windows"):
                    (root / directory).mkdir(parents=True)
                for name in ("Cargo.toml", "Cargo.lock", "build.rs", "src/main.rs", "packaging/windows/application.rc"):
                    (root / name).write_bytes(b"reviewed")
            checks.verify_integration(source, integrated)
            module = integrated / "src/duplicate_credentials.rs"
            module.write_bytes(b"duplicate")
            with self.assertRaisesRegex(RuntimeError, "build_inputs_differ"):
                checks.verify_integration(source, integrated)
            (integrated / "Cargo.lock").write_bytes(b"reviewed")
            (integrated / "assets").rmdir()
            (integrated / "assets").symlink_to(source / "assets", target_is_directory=True)
            with self.assertRaisesRegex(RuntimeError, "symlink_refused"):
                checks.verify_integration(source, integrated)
            (integrated / "assets").unlink()
            (integrated / "assets").mkdir()
            module.unlink()
            (integrated / "Cargo.lock").write_bytes(b"different")
            with self.assertRaisesRegex(RuntimeError, "build_inputs_differ"):
                checks.verify_integration(source, integrated)

    def test_overlay_is_inside_tests_and_refuses_second_application(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "src").mkdir()
            original = b"// production\n#[cfg(test)] mod tests {\n}\n"
            module = root / "src/smtp_credentials.rs"
            module.write_bytes(original)
            fixture = root / "fixture.rs"
            fixture.write_bytes(b"// synthetic tests only")
            with patch.object(checks, "MODULE_SHA256", checks.hashlib.sha256(original).hexdigest()):
                self.assertEqual(checks.overlay(root, fixture), original)
                self.assertTrue(module.read_bytes().startswith(original.rstrip()[:-1]))
                self.assertEqual((root / "src/macos_smtp_native_fixture.rs").read_bytes(), fixture.read_bytes())
                with self.assertRaisesRegex(RuntimeError, "hash_mismatch"):
                    checks.overlay(root, fixture)

    def test_wrong_platform_refused_before_source_or_keychain_access(self):
        with patch.object(checks.sys, "platform", "linux"):
            with self.assertRaisesRegex(RuntimeError, "requires_expected"):
                checks.main()

    def test_unreviewed_module_refused_without_writing(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "src").mkdir()
            module = root / "src/smtp_credentials.rs"
            module.write_bytes(b"unreviewed")
            with self.assertRaisesRegex(RuntimeError, "hash_mismatch"):
                checks.overlay(root, root / "missing-fixture")
            self.assertEqual(module.read_bytes(), b"unreviewed")

    def test_nonzero_command_is_not_pass(self):
        with self.assertRaisesRegex(RuntimeError, "operation_failed"):
            checks.run([sys.executable, "-c", "raise SystemExit(3)"], Path.cwd(), dict(os.environ), 5, [])

    def test_secret_output_is_rejected(self):
        for secret in checks.CANARIES:
            with self.subTest(secret=secret), self.assertRaisesRegex(RuntimeError, "plaintext_detected"):
                checks.run([sys.executable, "-c", "print(" + repr(secret.decode()) + ")"], Path.cwd(), dict(os.environ), 5, [])

    def test_timeout_is_not_pass(self):
        with self.assertRaisesRegex(RuntimeError, "timeout_process_group_killed_keychain_cleanup_unknown"):
            checks.run([sys.executable, "-c", "import time; time.sleep(20)"], Path.cwd(), dict(os.environ), 0.05, [])

    def test_success_output_is_captured(self):
        logs = []
        self.assertEqual(checks.run([sys.executable, "-c", "print('safe')"], Path.cwd(), dict(os.environ), 5, logs), b"safe\n")
        self.assertEqual(logs, [b"safe\n"])


if __name__ == "__main__":
    unittest.main()
