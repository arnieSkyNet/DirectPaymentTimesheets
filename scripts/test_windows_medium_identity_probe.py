"""Static approval-command contracts only. Never execute PowerShell/the probe."""
import base64
import hashlib
import os
from pathlib import Path
import re
import subprocess
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
PROBE = ROOT / 'scripts/windows_medium_identity_probe.ps1'
COMMAND = ROOT / 'scripts/windows_medium_identity_probe.command.txt'


class ProbeApprovalContract(unittest.TestCase):
    def test_approved_command_is_exact_reviewed_probe_and_has_no_bypass(self):
        text = COMMAND.read_text()
        prefix = r'C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe -NoLogo -NoProfile -NonInteractive -EncodedCommand '
        self.assertTrue(text.startswith(prefix))
        self.assertLess(len(text.strip()), 8191)
        decoded = base64.b64decode(text[len(prefix):].strip(), validate=True).decode('utf-16le')
        self.assertEqual(decoded, '& {\n' + PROBE.read_text() + '\n} -ApprovedDesktopIdentityProbe')
        self.assertNotIn('ExecutionPolicy', text)

    def test_probe_default_disabled_and_refuses_remote_context_before_token_query(self):
        text = PROBE.read_text()
        self.assertIn('param([switch]$ApprovedDesktopIdentityProbe)', text)
        gate = text.index('if (-not $ApprovedDesktopIdentityProbe) { exit 2 }')
        remote = text.index('if ($env:SSH_CONNECTION -or $env:SSH_CLIENT -or -not [Environment]::UserInteractive) { exit 2 }')
        query = text.index('[Security.Principal.WindowsIdentity]::GetCurrent()')
        self.assertLess(gate, remote)
        self.assertLess(remote, query)

    def test_only_account_integrity_elevation_are_output(self):
        text = PROBE.read_text()
        self.assertIn('$result = [ordered]@{account=$null;integrity_level=$null;elevated=$null}', text)
        self.assertEqual(set(re.findall(r'\$result\.([a-z_]+)\s*=', text)), {'account','integrity_level','elevated'})
        self.assertEqual(text.count('ConvertTo-Json'), 1)
        self.assertNotRegex(text, r'(?i)Write-(Host|Output|Error|Warning|Verbose)|Out-File|Set-Content')

    def test_no_other_process_token_or_write_operations(self):
        text = PROBE.read_text()
        self.assertNotRegex(text, r'(?i)Add-Type|Start-Process|CreateProcess|DuplicateToken|Impersonat|OpenProcess|TokenLinkedToken|SetTokenInformation|Set-Acl|New-Item|Remove-Item|Invoke-Command|Invoke-WebRequest|Get-Credential')
        self.assertIn('[Enum]::ToObject($kind,25)', text)
        self.assertIn('[Enum]::ToObject($kind,20)', text)
        self.assertNotIn('[Enum]::ToObject($kind,19)', text)
        self.assertIn('$resource.Dispose()', text)

    def test_success_requires_exact_user_medium_and_not_elevated(self):
        text = PROBE.read_text()
        self.assertIn("$result.account -ieq 'IMOLLY\\mark'", text)
        self.assertNotIn("'IMOLLY\\\\mark'", text)
        self.assertIn("$result.integrity_level -eq 'MEDIUM'", text)
        self.assertIn('$result.elevated -eq $false', text)
        self.assertIn("'S-1-16-8192'='MEDIUM'", text)
        self.assertIn("'S-1-16-8448'='MEDIUM_PLUS'", text)
        self.assertEqual(text.count('$exitCode = 0'), 1)
        self.assertIn('exit $exitCode', text)


@unittest.skipUnless(os.environ.get('DPT_IMOLLY_SOURCE_ROOT'), 'Explicit isolated source candidate required')
class SourceByteContract(unittest.TestCase):
    def test_rust_change_is_only_the_two_hostname_literals(self):
        root = Path(os.environ['DPT_IMOLLY_SOURCE_ROOT'])
        name = 'tests/native_sqlite_terminal_fixtures.rs'
        old = subprocess.check_output(['git','--no-optional-locks','-C',str(root),'show',
                                      '0df2019b45a77995b869685621ea8420501f7956:'+name])
        expected = old.replace(b'Ok("Molly")',b'Ok("IMOLLY")').replace(
            b'.eq_ignore_ascii_case("Molly")',b'.eq_ignore_ascii_case("IMOLLY")')
        actual = (root/name).read_bytes()
        self.assertEqual(actual, expected)
        import native_sqlite_terminal_checks as n
        self.assertEqual(hashlib.sha256(actual).hexdigest(), n.REVIEWED_FIXTURE_SHA256)

    def test_old_published_fixture_is_refused_by_corrected_controller(self):
        import native_sqlite_terminal_checks as n
        root = Path(os.environ['DPT_IMOLLY_SOURCE_ROOT'])
        sha = '0df2019b45a77995b869685621ea8420501f7956'
        old_inventory = {n.FIXTURE: {'sha256': 'a2fe71e95c11e712f5c58459974e91869b44e1d9b832815a6d60f3a7d18168bc'}}
        with patch.object(n, 'source_inventory', return_value=old_inventory), patch.object(n.subprocess, 'check_output', side_effect=[sha, b'']), patch.object(n, 'execute') as execute:
            with self.assertRaisesRegex(n.Refusal, 'unreviewed_fixture'):
                n.verify_source_fixture(root, sha)
            execute.assert_not_called()

    def test_source_controller_split_and_documented_fingerprint(self):
        root = Path(os.environ['DPT_IMOLLY_SOURCE_ROOT'])
        self.assertFalse((ROOT/'tests/native_sqlite_terminal_fixtures.rs').exists())
        self.assertEqual((ROOT/'tests/native_sqlite_terminal_fixtures.md').read_bytes(),
                         (root/'tests/native_sqlite_terminal_fixtures.md').read_bytes())
        import native_sqlite_terminal_checks as n
        self.assertIn(n.REVIEWED_FIXTURE_SHA256, (root/'tests/native_sqlite_terminal_fixtures.md').read_text())


if __name__ == '__main__':
    unittest.main()
