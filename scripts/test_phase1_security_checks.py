"""Test the runner's failure accounting without running cargo or touching data."""
import contextlib
import io
import os
import subprocess
import unittest
from unittest.mock import patch

import phase1_security_checks as checks

LISTING = "phase1_windows_actual_new_object_owners_match_current_token: test\n"


class RunnerTests(unittest.TestCase):
    def invoke(self, run):
        with patch.object(checks.platform, "system", return_value="Windows"), \
                patch.object(checks.os, "chdir"), \
                patch.dict(os.environ, {"EXPECTED_SHA": "a" * 40}), \
                patch.object(checks.subprocess, "check_output", return_value="a" * 40), \
                patch.object(checks.subprocess, "run", side_effect=run), \
                contextlib.redirect_stdout(io.StringIO()), \
                contextlib.redirect_stderr(io.StringIO()):
            return checks.main()

    def test_zero_matching_tests_cannot_pass(self):
        def run(args, **kwargs):
            return subprocess.CompletedProcess(args, 0, "", "")
        self.assertEqual(self.invoke(run), 1)

    def test_failure_does_not_hide_later_groups(self):
        executed = []
        def run(args, **kwargs):
            if "--list" in args:
                return subprocess.CompletedProcess(args, 0, LISTING, "")
            if "test" in args:
                executed.append(args[4])
                return subprocess.CompletedProcess(args, int(args[4] == "phase1"))
            return subprocess.CompletedProcess(args, 0)
        self.assertEqual(self.invoke(run), 1)
        self.assertEqual(executed, list(checks.GROUPS))

    def test_success_requires_every_discovered_group_to_pass(self):
        def run(args, **kwargs):
            return subprocess.CompletedProcess(args, 0, LISTING, "")
        self.assertEqual(self.invoke(run), 0)

    def test_missing_native_marker_cannot_pass(self):
        def run(args, **kwargs):
            return subprocess.CompletedProcess(args, 0, "generic_fixture: test\n", "")
        self.assertEqual(self.invoke(run), 1)

    def test_wrong_revision_stops_before_compilation(self):
        with patch.object(checks.platform, "system", return_value="Windows"), \
                patch.object(checks.os, "chdir"), \
                patch.dict(os.environ, {"EXPECTED_SHA": "a" * 40}), \
                patch.object(checks.subprocess, "check_output", return_value="b" * 40), \
                patch.object(checks.subprocess, "run") as run, \
                contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(checks.main(), 2)
            run.assert_not_called()


if __name__ == "__main__":
    unittest.main()
