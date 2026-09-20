import importlib.util
import json
import os
import platform
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('package', ROOT / 'packaging/package.py')
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


class PackagingTests(unittest.TestCase):
    def test_explicit_target_mapping_and_nine_unique_packages(self):
        names = package.expected_packages('1.0.1')
        self.assertEqual(len(names), 9)
        self.assertEqual(len(set(names.values())), 9)
        self.assertIn('direct-payment-timesheets_1.0.1_armhf.deb', names)
        self.assertEqual(names['DirectPaymentTimesheets-1.0.1-linux-armhf.AppImage'], ('armv7-unknown-linux-gnueabihf', 'appimage'))
        self.assertIn('DirectPaymentTimesheets-1.0.1-macos-arm64.dmg', names)

    def test_elf_rejects_wrong_arch_soft_float_and_new_glibc(self):
        arm = 'Class: ELF32\nMachine: ARM\nFlags: hard-float ABI\nTag_CPU_arch: v7\nTag_ABI_VFP_args: VFP registers\nGLIBC_2.36'
        package.validate_elf(arm, 'armv7-unknown-linux-gnueabihf')
        for bad in [arm.replace('hard-float', 'soft-float'), arm.replace('v7', 'v6'), arm.replace('VFP registers', 'base'), arm.replace('2.36', '2.38')]:
            with self.assertRaises(ValueError): package.validate_elf(bad, 'armv7-unknown-linux-gnueabihf')
        with self.assertRaises(ValueError): package.validate_elf(arm, 'aarch64-unknown-linux-gnu')
        package.validate_elf('Class: ELF64\nMachine: AArch64\nGLIBC_2.17', 'aarch64-unknown-linux-gnu')
        package.validate_elf('Class: ELF64\nMachine: Advanced Micro Devices X86-64\nGLIBC_2.34', 'x86_64-unknown-linux-gnu')

    def test_license_staging_includes_embedded_font_and_native_notices(self):
        with tempfile.TemporaryDirectory() as tmp:
            package.licenses(Path(tmp))
            for path in ['LICENSE', 'DejaVu-LICENSE.txt', 'THIRD_PARTY_LICENSES.md', 'third-party/fonts/OFL.txt', 'third-party/native/libsqlite3-sys-MIT.txt']:
                self.assertTrue((Path(tmp) / path).is_file(), path)

    def fixture(self, directory):
        for index, (name, (target, kind)) in enumerate(package.expected_packages('1.0.1').items()):
            path = directory / str(index) / name
            path.parent.mkdir()
            path.write_bytes(b'disposable package')
            data = dict(source_sha='a' * 40, version='1.0.1', target=target, installation_kind=kind, file=name, sha256=package.digest(path))
            path.with_name(name + '.json').write_text(json.dumps(data))
            path.with_name(name + '.sha256').write_text(f"{data['sha256']}  {name}\n")

    def test_collection_checks_and_flattens_all_nine_packages(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); self.fixture(root)
            package.collect(root, 'a' * 40, '1.0.1')
            self.assertEqual(len(json.loads((root / 'rehearsal-manifest.json').read_text())), 9)
            for line in (root / 'SHA256SUMS').read_text().splitlines():
                sha, name = line.split('  ')
                self.assertEqual(package.digest(root / name), sha)

    def test_collection_refuses_missing_duplicate_modified_and_mixed_source(self):
        for defect in ['missing', 'duplicate', 'modified', 'source', 'kind', 'version', 'checksum']:
            with self.subTest(defect=defect), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp); self.fixture(root)
                path = next(root.rglob('*.deb'))
                if defect == 'missing': path.unlink()
                elif defect == 'duplicate': (root / path.name).write_bytes(path.read_bytes())
                elif defect == 'modified': path.write_bytes(b'changed')
                elif defect == 'checksum': path.with_name(path.name + '.sha256').write_text('wrong')
                else:
                    meta = path.with_name(path.name + '.json')
                    data = json.loads(meta.read_text())
                    data[{'source': 'source_sha', 'kind': 'installation_kind', 'version': 'version'}[defect]] = 'wrong'
                    meta.write_text(json.dumps(data))
                with self.assertRaises((ValueError, FileNotFoundError)): package.collect(root, 'a' * 40, '1.0.1')
                self.assertFalse((root / 'rehearsal-manifest.json').exists())

    def test_binary_handoff_refuses_wrong_kind_or_changed_binary(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(package, 'source_sha', return_value='b' * 40):
            binary = Path(tmp) / 'binary'; binary.write_bytes(b'test')
            with patch.object(package.subprocess, 'check_output', return_value='rustc 1.98.1 (test)'):
                package.binary_provenance(binary, 'armv7-unknown-linux-gnueabihf', 'deb')
            package.binary_provenance(binary, 'armv7-unknown-linux-gnueabihf', 'deb', verify=True)
            with self.assertRaises(ValueError): package.binary_provenance(binary, 'armv7-unknown-linux-gnueabihf', 'appimage', verify=True)
            binary.write_bytes(b'modified')
            with self.assertRaises(ValueError): package.binary_provenance(binary, 'armv7-unknown-linux-gnueabihf', 'deb', verify=True)

    @unittest.skipUnless(platform.system() == 'Linux' and platform.machine() == 'x86_64' and all(shutil.which(tool) for tool in ['gcc', 'dpkg-deb', 'dpkg-shlibdeps', 'readelf', 'strip']), 'Linux packaging tools required')
    def test_deb_packaging_with_disposable_synthetic_elf(self):
        # Tests the real Debian script without building or launching the application.
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / 'stub.c'; source.write_text('int main(void) { return 0; }\n')
            binary = root / 'stub'
            subprocess.run(['gcc', str(source), '-o', str(binary)], check=True)
            package.validate_elf(subprocess.check_output(['readelf', '-h', '-A', '--version-info', str(binary)], text=True), 'x86_64-unknown-linux-gnu')
            binary.with_name(binary.name + '.build.json').write_text(json.dumps(dict(source_sha=package.source_sha(), version=package.version(), target='x86_64-unknown-linux-gnu', installation_kind='deb', sha256=package.digest(binary), rustc='synthetic C fixture, not an application build')))
            env = dict(os.environ, TARGET='x86_64-unknown-linux-gnu', SKIP_BUILD='1', SOURCE_BINARY=str(binary), OUTPUT_DIRECTORY=str(root / 'dist'), STRIP='strip')
            result = subprocess.run(['bash', str(ROOT / 'packaging/linux/build-deb.sh')], env=env, text=True, capture_output=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            deb = next((root / 'dist').glob('*.deb'))
            self.assertEqual(subprocess.check_output(['dpkg-deb', '-f', str(deb), 'Architecture'], text=True).strip(), 'amd64')
            metadata = json.loads(deb.with_name(deb.name + '.json').read_text())
            self.assertEqual(metadata['sha256'], package.digest(deb))
            self.assertEqual(metadata['installation_kind'], 'deb')
            # Same executable must not be mislabelled as an AppImage build.
            result = subprocess.run(['bash', str(ROOT / 'packaging/linux/build-appimage.sh')], env=env, text=True, capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('Wrong source, target, installation kind', result.stderr)

    def test_tools_have_fixed_urls_and_sha256_for_every_architecture(self):
        tools = json.loads((ROOT / 'packaging/linux/tools.json').read_text())
        self.assertEqual(set(tools), {'x86_64', 'armhf', 'aarch64'})
        for values in tools.values():
            self.assertEqual(set(values), {'linuxdeploy', 'appimagetool', 'runtime', 'runtime-license'})
            for url, sha in values.values():
                self.assertNotIn('/continuous/', url)
                self.assertNotIn('/latest/', url)
                self.assertRegex(sha, r'^[0-9a-f]{64}$')
                self.assertTrue(url.startswith(('https://github.com/', 'https://raw.githubusercontent.com/')))

    def test_workflows_are_artifact_only_with_read_only_permissions_and_pins(self):
        for file in (ROOT / '.github/workflows').glob('*.yml'):
            text = file.read_text()
            self.assertIn('contents: read', text)
            self.assertNotRegex(text, r'(?m)^\s+(push|release|tags):')
            self.assertNotIn('contents: write', text)
            self.assertNotRegex(text, r'(?m)^\s+run:.*gh release')
            for action in re.findall(r'uses:\s+(\S+)', text):
                if not action.startswith('./'):
                    self.assertRegex(action, r'@[0-9a-f]{40}$')
        workflow = (ROOT / '.github/workflows/linux-packages-test.yml').read_text()
        for sha in json.loads((ROOT / 'packaging/toolchain.json').read_text())['container_digests'].values():
            self.assertIn(sha, workflow)

    def test_windows_guard_precedes_install_and_preserves_user_data_rules(self):
        text = (ROOT / 'packaging/windows/installer.nsi').read_text()
        init = text.split('Function .onInit', 1)[1].split('FunctionEnd', 1)[0]
        self.assertIn('${AtLeastWin10}', init)
        self.assertIn('${RunningX64}', init)
        self.assertIn('ManifestSupportedOS all', text)
        self.assertIn('RequestExecutionLevel user', text)
        self.assertNotIn('RMDir /r', text)

    def test_linux_scripts_reject_missing_or_unknown_target_before_building(self):
        if os.name == 'nt': self.skipTest('bash checks run on Linux')
        for target in ['', 'unsupported']:
            env = dict(os.environ, TARGET=target)
            proc = subprocess.run(['bash', str(ROOT / 'packaging/linux/build-deb.sh')], env=env, capture_output=True, text=True)
            self.assertNotEqual(proc.returncode, 0)
            self.assertIn('target', proc.stderr.lower())


if __name__ == '__main__':
    unittest.main()
