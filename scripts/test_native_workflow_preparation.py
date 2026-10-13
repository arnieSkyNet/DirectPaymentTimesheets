"""Safe workflow mocks: no native process, signal or SQLite execution."""
import contextlib
import io
import json
import pathlib
import subprocess
import os
from pathlib import Path
import re
import runpy
import sys
import textwrap
import unittest
from unittest.mock import patch, MagicMock, Mock
from phase1_security_checks import ContainmentError

WORKFLOW = (Path(__file__).resolve().parents[1] / '.github/workflows/phase1-security-native.yml').read_text()
BLOCKS = [textwrap.dedent(code) for code in re.findall(r"python -I -B - <<'PYTHON'\n(.*?)\n          PYTHON", WORKFLOW, re.S)]
NAMES = ('test_breakaway_is_refused', 'test_child_grandchild_timeout_releases_exclusive_handle', 'test_normal_exit_cleans_descendant')

class WorkflowMocks(unittest.TestCase):
    def controls(self, failure=None, position=None, mac=False, reason='Requires native Windows Job Objects'):
        seen = []
        def method(index):
            def control(case):
                seen.append(index)
                if mac or index == position:
                    raise (unittest.SkipTest if mac else failure)(reason)
            return control
        cls = type('NativeContainmentTests', (unittest.TestCase,), {name: method(i) for i, name in enumerate(NAMES)})
        cls.__module__ = 'test_phase1_security_checks'
        suite = unittest.defaultTestLoader.loadTestsFromTestCase(cls)
        with patch.dict(os.environ, {'NATIVE_RUNNER': 'macos-15' if mac else 'windows-2022'}), patch.object(os, 'name', 'posix' if mac else 'nt'), patch.object(unittest.defaultTestLoader, 'discover', return_value=suite), contextlib.redirect_stderr(io.StringIO()):
            try:
                exec(compile(BLOCKS[0], 'workflow', 'exec'), {})
            except SystemExit:
                return seen, False
        return seen, True

    def test_failures_errors_and_skips_stop_at_each_position(self):
        for failure in (ContainmentError, AssertionError, RuntimeError, unittest.SkipTest):
            for position in range(3):
                with self.subTest(failure=failure, position=position):
                    self.assertEqual(self.controls(failure, position), (list(range(position+1)), False))

    def test_success_and_exact_macos_skips(self):
        self.assertEqual(self.controls(), ([0, 1, 2], True))
        self.assertEqual(self.controls(mac=True), ([0, 1, 2], True))
        self.assertEqual(self.controls(mac=True, reason='unexpected'), ([0], False))

    def test_safe_environment_and_arguments_are_fixed(self):
        with patch.dict(os.environ, {'EXPECTED_SHA': 'a'*40, 'NATIVE_RUNNER': 'macos-15', 'DPT_SYNTHETIC_SQLITE_TERMINAL_TESTS': '1', 'DPT_WINDOWS_ROOT_DIAGNOSTICS': '1', 'DPT_NATIVE_SQLITE_CASE': 'terminal'}, clear=True), patch.object(sys, 'path', list(sys.path)), patch.object(sys, 'argv', ['injected', '--run']), patch.object(runpy, 'run_path') as launch:
            exec(compile(BLOCKS[-1], 'workflow', 'exec'), {})
            self.assertEqual(sys.argv, ['native_sqlite_terminal_checks.py', '--containment-only'])
            self.assertEqual(os.environ['DPT_NATIVE_SQLITE_SAFE_CONTAINMENT'], '1')
            for key in ('DPT_SYNTHETIC_SQLITE_TERMINAL_TESTS', 'DPT_WINDOWS_ROOT_DIAGNOSTICS', 'DPT_NATIVE_SQLITE_CASE'):
                self.assertNotIn(key, os.environ)
            launch.assert_called_once_with('scripts/native_sqlite_terminal_checks.py', run_name='__main__')

    def test_inventory_listing_is_fixed_and_fail_closed(self):
        target = MagicMock(spec=Path)
        target.resolve.return_value = target
        target.is_relative_to.return_value = target.is_file.return_value = True
        record = dict(reason='compiler-artifact', target=dict(name='native_sqlite_terminal_fixtures', kind=['test']), profile=dict(test=True), executable='synthetic')
        target.__truediv__.return_value.read_text.return_value = json.dumps(record)
        for status, output, error, accepted in ((0, 'native_child: test\n\n1 test, 0 benchmarks\n', '', True), (1, '', '', False), (0, '', '', False), (0, 'native_child: test\n1 test, 0 benchmarks', 'unexpected', False)):
            with self.subTest(status=status, accepted=accepted), patch.object(pathlib, 'Path', return_value=target), patch.object(subprocess, 'run', return_value=Mock(returncode=status, stdout=output, stderr=error)) as child, contextlib.redirect_stdout(io.StringIO()):
                if accepted:
                    exec(compile(BLOCKS[1], 'workflow', 'exec'), {})
                else:
                    with self.assertRaises(SystemExit): exec(compile(BLOCKS[1], 'workflow', 'exec'), {})
                self.assertEqual(child.call_args.args[0][1:], ['--list', '--ignored'])
                self.assertEqual(child.call_args.kwargs['timeout'], 10)
        with patch.object(pathlib, 'Path', return_value=target), patch.object(subprocess, 'run', side_effect=subprocess.TimeoutExpired('synthetic', 10)):
            with self.assertRaises(SystemExit): exec(compile(BLOCKS[1], 'workflow', 'exec'), {})

    def test_embedded_python_has_no_assert_security_checks(self):
        self.assertEqual(len(BLOCKS), 3)
        for code in BLOCKS:
            compile(code, 'workflow', 'exec')
            self.assertNotIn('assert ', code)

if __name__ == '__main__':
    unittest.main()
