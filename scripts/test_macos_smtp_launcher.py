"""Synthetic launcher tests; Git identities are mocked, no native store calls."""
import hashlib
import importlib.util
import os
from pathlib import Path
import tempfile
import types
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("macos_smtp_launcher", Path(__file__).with_name("macos_smtp_launcher.py"))
launcher = importlib.util.module_from_spec(spec)
spec.loader.exec_module(launcher)
CHECKOUT_IDENTITY = launcher.checkout_identity
CHECKS_MODULE = launcher.checks_module


class LauncherTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.roots = tuple(Path(self.temp.name) / name for name in ("controller", "harness", "source"))
        self.controller, self.harness, self.source = self.roots
        for root in self.roots:
            (root / "scripts").mkdir(parents=True)
        for root in (self.harness, self.source):
            (root / "build-input").write_bytes(b"reviewed")
        self.fixture = self.harness / "scripts/macos_smtp_native_fixture.rs"
        self.fixture.write_bytes(b"synthetic fixture")
        for name in launcher.OVERLAY_FILES:
            (self.controller / "scripts" / name).write_bytes(("reviewed " + name).encode())
        for name in ("macos_smtp_checks.py", "test_macos_smtp_checks.py"):
            (self.harness / "scripts" / name).write_bytes(b"old test support")
        self.addCleanup(patch.stopall)
        patch.dict(os.environ, GITHUB_EVENT_NAME="workflow_dispatch", GITHUB_REF="refs/heads/main", GITHUB_SHA="a" * 40).start()
        self.identities = patch.object(launcher, "checkout_identity").start()
        self.checked = []
        def verify(root):
            self.checked.append(root)
            if (root / "build-input").read_bytes() != b"reviewed":
                raise RuntimeError("build_inputs_differ_from_original_reviewed_source")
        self.checks = types.SimpleNamespace(
            load_reviewed_manifest=lambda: dict.fromkeys(range(111)),
            verify_reviewed_source=verify,
            verify_integration=lambda a, b: None,
        )
        patch.object(launcher, "checks_module", return_value=self.checks).start()
        patch.object(launcher, "FIXTURE_SHA256", hashlib.sha256(self.fixture.read_bytes()).hexdigest()).start()

    def snapshot(self, root):
        return {p.relative_to(root).as_posix(): p.read_bytes() for p in root.rglob("*") if p.is_file()}

    def test_checkout_identity_enforces_head_cleanliness_and_deadline(self):
        def result(stdout, code=0, stderr=b""):
            return types.SimpleNamespace(stdout=stdout, returncode=code, stderr=stderr)
        expected = launcher.SOURCE_SHA
        for outputs, error in (([result(b"wrong\n")], "checkout_sha_mismatch"),
                               ([result((expected + "\n").encode()), result(b" M file\n")], "checkout_not_clean"),
                               ([result(b"", 1)], "checkout_identity_unconfirmed")):
            with self.subTest(error=error), patch.object(launcher.subprocess, "run", side_effect=outputs):
                with self.assertRaisesRegex(RuntimeError, error):
                    CHECKOUT_IDENTITY(self.source, expected)
        with patch.object(launcher.subprocess, "run", side_effect=launcher.subprocess.TimeoutExpired("git", 10)):
            with self.assertRaisesRegex(RuntimeError, "identity_unconfirmed"):
                CHECKOUT_IDENTITY(self.source, expected)

    def test_symlink_controller_input_refused(self):
        path = self.controller / "scripts/macos_smtp_checks.py"
        path.unlink()
        path.symlink_to(self.source / "build-input")
        with self.assertRaisesRegex(RuntimeError, "file_missing_or_symlink"):
            launcher.prepare(*self.roots)

    def test_symlink_checkout_root_refused(self):
        alias = Path(self.temp.name) / "alias"
        alias.symlink_to(self.source, target_is_directory=True)
        with self.assertRaisesRegex(RuntimeError, "root_symlink_refused"):
            launcher.prepare(self.controller, self.harness, alias)

    def test_controller_module_pin_and_symlink_checked_before_use(self):
        path = self.controller / "scripts/macos_smtp_checks.py"
        path.write_text("SOURCE_SHA = 'wrong'\n")
        with self.assertRaisesRegex(RuntimeError, "controller_source_pin_mismatch"):
            CHECKS_MODULE(self.controller)
        path.unlink()
        path.symlink_to(self.source / "build-input")
        with self.assertRaisesRegex(RuntimeError, "file_missing_or_symlink"):
            CHECKS_MODULE(self.controller)

    def test_main_controller_only_exact_three_file_overlay(self):
        original_controller = self.snapshot(self.controller)
        original_source = self.snapshot(self.source)
        original_fixture = self.fixture.read_bytes()
        launcher.prepare(*self.roots)
        self.assertEqual(self.snapshot(self.controller), original_controller)
        self.assertEqual(self.snapshot(self.source), original_source)
        self.assertEqual(self.fixture.read_bytes(), original_fixture)
        for name in launcher.OVERLAY_FILES:
            self.assertEqual((self.harness / "scripts" / name).read_bytes(), (self.controller / "scripts" / name).read_bytes())
        self.assertNotIn(self.controller, self.checked)
        self.assertEqual(self.identities.call_args_list[0].args, (self.controller, "a" * 40))
        self.assertEqual(self.identities.call_args_list[1].args, (self.harness, launcher.SOURCE_SHA))
        self.assertEqual(self.identities.call_args_list[2].args, (self.source, launcher.SOURCE_SHA))

    def test_non_main_or_non_manual_event_refused_before_git(self):
        for changes in ({"GITHUB_REF": "refs/heads/security/v1.0.7-phase1"}, {"GITHUB_EVENT_NAME": "push"}, {"GITHUB_SHA": "main"}):
            with self.subTest(changes=changes), patch.dict(os.environ, changes):
                with self.assertRaisesRegex(RuntimeError, "requires_manual_main"):
                    launcher.prepare(*self.roots)
        self.identities.assert_not_called()

    def test_identity_failure_stops_before_any_copy(self):
        before = self.snapshot(self.harness)
        self.identities.side_effect = RuntimeError("checkout_sha_mismatch")
        with self.assertRaisesRegex(RuntimeError, "checkout_sha_mismatch"):
            launcher.prepare(*self.roots)
        self.assertEqual(self.snapshot(self.harness), before)

    def test_manifest_drift_in_either_tree_stops_before_copy(self):
        before = self.snapshot(self.harness)
        for root in (self.harness, self.source):
            with self.subTest(root=root):
                (root / "build-input").write_bytes(b"drift")
                with self.assertRaisesRegex(RuntimeError, "differ_from_original"):
                    launcher.prepare(*self.roots)
                (root / "build-input").write_bytes(b"reviewed")
                self.assertEqual(self.snapshot(self.harness), before)

    def test_wrong_manifest_count_refused_before_copy(self):
        self.checks.load_reviewed_manifest = lambda: {}
        with self.assertRaisesRegex(RuntimeError, "input_count_mismatch"):
            launcher.prepare(*self.roots)

    def test_fixture_drift_refused_before_copy(self):
        self.fixture.write_bytes(b"changed")
        before = self.snapshot(self.harness)
        with self.assertRaisesRegex(RuntimeError, "native_fixture_hash_mismatch"):
            launcher.prepare(*self.roots)
        self.assertEqual(self.snapshot(self.harness), before)

    def test_symlink_overlay_destination_refused(self):
        dst = self.harness / "scripts/macos_smtp_checks.py"
        dst.unlink()
        dst.symlink_to(self.source / "build-input")
        with self.assertRaisesRegex(RuntimeError, "overlay_destination_refused"):
            launcher.prepare(*self.roots)
        self.assertEqual((self.source / "build-input").read_bytes(), b"reviewed")

    def test_overlapping_roots_refused(self):
        with self.assertRaisesRegex(RuntimeError, "roots_not_disjoint"):
            launcher.prepare(self.controller, self.harness, self.harness)

    def test_source_identity_rechecked_after_copy(self):
        self.identities.side_effect = [None, None, None, RuntimeError("checkout_not_clean")]
        with self.assertRaisesRegex(RuntimeError, "checkout_not_clean"):
            launcher.prepare(*self.roots)


if __name__ == "__main__":
    unittest.main()
