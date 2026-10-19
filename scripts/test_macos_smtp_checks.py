"""Harness tests use fake commands/files only; these are NOT Keychain tests."""
import importlib.util
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
