import hashlib
from contextlib import nullcontext
import importlib.util
import io
import json
import os
import sys
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch

from check_dependency_advisories import matches
from third_party_licenses import generate, LEGACY_NOTICES

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('review_package', ROOT / 'packaging/package.py')
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)
vendor_spec = importlib.util.spec_from_file_location('offline_vendor', ROOT / 'packaging/offline_vendor.py')
vendor = importlib.util.module_from_spec(vendor_spec)
vendor_spec.loader.exec_module(vendor)

source_spec = importlib.util.spec_from_file_location('source_bundle_review', ROOT / 'packaging/source_bundle.py')
source_bundle = importlib.util.module_from_spec(source_spec)
with patch.dict(sys.modules, {'package': package}):
    source_spec.loader.exec_module(source_bundle)


class DependencyReviewTests(unittest.TestCase):
    def notice_fixture(self, root):
        # Every byte is fictional, including the independent archive source.
        for name in (*LEGACY_NOTICES, 'LICENSE', 'assets/fonts/DejaVu-LICENSE.txt'):
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b'Synthetic licence attribution')
        cache = root / 'cargo/registry/cache/synthetic'
        cache.mkdir(parents=True)
        archive = cache / 'fixture-1.0.0.crate'
        with tarfile.open(archive, 'w:gz') as tar:
            for name, data in [('Cargo.toml', b'[package]\nname="fixture"\nversion="1.0.0"\nlicense="MIT"\n'),
                               ('vendor/COPYING', b'Synthetic native attribution')]:
                member = tarfile.TarInfo('fixture-1.0.0/' + name)
                member.size = len(data)
                tar.addfile(member, io.BytesIO(data))
        checksum = hashlib.sha256(archive.read_bytes()).hexdigest()
        (root / 'Cargo.lock').write_text('[[package]]\nname="fixture"\nversion="1.0.0"\nsource="registry+https://github.com/rust-lang/crates.io-index"\nchecksum="' + checksum + '"\n')
        for name, data in generate(root, root / 'cargo').items():
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)

    def test_packaging_rejects_missing_inventory_and_modified_native_notice(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(package, 'ROOT', Path(tmp)):
            root = Path(tmp)
            self.notice_fixture(root)
            package.verify_license_inventory()
            notice = root / 'third-party/crates/fixture-1.0.0/vendor/COPYING'
            notice.write_bytes(b'changed synthetic attribution')
            with self.assertRaisesRegex(ValueError, 'attribution'):
                package.verify_license_inventory()

    def test_empty_and_omitted_manifest_notice_maps_fail_closed(self):
        for mutation in ('empty notices', 'missing notices', 'empty packages', 'missing inventory', 'missing attribution'):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as tmp, patch.object(package, 'ROOT', Path(tmp)):
                root = Path(tmp)
                self.notice_fixture(root)
                manifest = root / 'third-party/crates/manifest.json'
                data = json.loads(manifest.read_text())
                if mutation == 'empty notices':
                    data['packages'][0]['notices'] = {}
                elif mutation == 'missing notices':
                    del data['packages'][0]['notices']
                elif mutation == 'empty packages':
                    data['packages'] = []
                elif mutation == 'missing inventory':
                    manifest.unlink()
                else:
                    (root / 'third-party/crates/fixture-1.0.0/vendor/COPYING').unlink()
                    data['packages'][0]['notices'].pop('vendor/COPYING')
                if mutation != 'missing inventory':
                    manifest.write_text(json.dumps(data))
                with self.assertRaises((ValueError, FileNotFoundError)):
                    package.verify_license_inventory()

    def test_empty_notice_maps_fail_even_with_updated_manifest_hash(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(package, 'ROOT', Path(tmp)):
            root = Path(tmp)
            self.notice_fixture(root)
            manifest = root / 'third-party/crates/manifest.json'
            data = json.loads(manifest.read_text())
            data['packages'][0]['notices'] = {}
            manifest.write_text(json.dumps(data))
            inventory = root / 'packaging/notice-allowlist.json'
            data = json.loads(inventory.read_text())
            data['files']['third-party/crates/manifest.json'] = package.digest(manifest)
            inventory.write_text(json.dumps(data))
            with self.assertRaisesRegex(ValueError, 'required upstream attribution'):
                package.verify_license_inventory()

    def test_required_inventories_and_attribution_cannot_be_omitted(self):
        for name in ('LICENSE', 'THIRD_PARTY_LICENSES.md', 'assets/fonts/DejaVu-LICENSE.txt',
                     LEGACY_NOTICES[0], 'packaging/notice-allowlist.json'):
            with self.subTest(name=name), tempfile.TemporaryDirectory() as tmp, patch.object(package, 'ROOT', Path(tmp)):
                root = Path(tmp)
                self.notice_fixture(root)
                (root / name).unlink()
                with self.assertRaises((ValueError, FileNotFoundError)):
                    package.verify_license_inventory()
        with tempfile.TemporaryDirectory() as tmp, patch.object(package, 'ROOT', Path(tmp)):
            root = Path(tmp)
            self.notice_fixture(root)
            path = root / 'packaging/notice-allowlist.json'
            data = json.loads(path.read_text())
            data['files'] = {}
            path.write_text(json.dumps(data))
            with self.assertRaisesRegex(ValueError, 'Missing required'):
                package.verify_license_inventory()

    def test_notice_staging_rejects_ignored_and_untracked_files_before_copy(self):
        ignored = (ROOT / '.gitignore').read_text().splitlines()
        self.assertIn('*.key', ignored)
        self.assertIn('*.db', ignored)
        for name in ('synthetic.key', 'synthetic.db', 'unexpected.txt'):
            with self.subTest(name=name), tempfile.TemporaryDirectory() as tmp, patch.object(package, 'ROOT', Path(tmp)):
                root = Path(tmp)
                self.notice_fixture(root)
                # Extensions ignored by this project's Git rules must still be rejected.
                (root / 'third-party' / name).write_bytes(b'Fictional private fixture')
                destination = root / 'installer-notices'
                with self.assertRaisesRegex(ValueError, 'unexpected notice files'):
                    package.licenses(destination)
                self.assertFalse(destination.exists())

    @unittest.skipUnless(hasattr(os, 'mkfifo'), 'Unix special-file regression')
    def test_notice_validation_refuses_fifo_and_symlink(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(package, 'ROOT', Path(tmp)):
            root = Path(tmp)
            self.notice_fixture(root)
            path = root / 'third-party/crates/fixture-1.0.0/vendor/COPYING'
            path.unlink()
            os.mkfifo(path, 0o600)
            with self.assertRaisesRegex(ValueError, 'Non-regular'):
                package.verify_license_inventory()
            path.unlink()
            path.symlink_to(root / 'LICENSE')
            with self.assertRaisesRegex(ValueError, 'Non-regular'):
                package.verify_license_inventory()

    def test_exact_staging_and_rejection_of_stale_destination_files(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(package, 'ROOT', Path(tmp)):
            root = Path(tmp)
            self.notice_fixture(root)
            destination = root / 'installer-notices'
            package.licenses(destination)
            allowed = set(package.verify_license_inventory())
            self.assertEqual(package.notice_tree_files(destination / 'third-party'), allowed)
            for relative in allowed:
                self.assertEqual((destination / 'third-party' / relative).read_bytes(),
                                 (root / 'third-party' / relative).read_bytes())
            (destination / 'third-party/synthetic.db').write_bytes(b'Fictional stale fixture')
            with self.assertRaisesRegex(ValueError, 'staging destination'):
                package.licenses(destination)

    def test_replacement_and_post_copy_tampering_abort_staging(self):
        for mutation in ('replacement', 'symlink replacement', 'post-copy tampering'):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as tmp, patch.object(package, 'ROOT', Path(tmp)):
                root = Path(tmp)
                self.notice_fixture(root)
                source = root / 'third-party/crates/fixture-1.0.0/vendor/COPYING'
                original = source.read_bytes()
                fictional = root / 'synthetic.key'
                fictional.write_bytes(b'Fictional replacement bytes')
                copy = package.safe_copy

                def replacing_copy(src, dst, *args, **kwargs):
                    if Path(src) != source:
                        return copy(src, dst, *args, **kwargs)
                    try:
                        if mutation == 'replacement':
                            source.write_bytes(b'Fictional concurrent replacement')
                        elif mutation == 'symlink replacement':
                            source.unlink()
                            source.symlink_to(fictional)
                        result = copy(src, dst, *args, **kwargs)
                        if mutation == 'post-copy tampering':
                            Path(dst).write_bytes(b'Fictional post-copy tampering')
                        return result
                    finally:
                        if source.is_symlink():
                            source.unlink()
                        source.write_bytes(original)

                with patch.object(package, 'safe_copy', side_effect=replacing_copy):
                    with self.assertRaises((ValueError, OSError)):
                        package.licenses(root / 'installer-notices')
                # Even a transient replacement restored before provenance fails.
                package.verify_license_inventory()
                self.assertEqual(source.read_bytes(), original)

    def test_staged_inventory_rejects_tampering_and_unexpected_files(self):
        for mutation in ('modified', 'symlink', 'unexpected.key', 'unexpected.db'):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as tmp, patch.object(package, 'ROOT', Path(tmp)):
                root = Path(tmp)
                self.notice_fixture(root)
                destination = root / 'installer-notices'
                package.licenses(destination)
                path = destination / 'third-party/crates/fixture-1.0.0/vendor/COPYING'
                if mutation == 'modified':
                    path.write_bytes(b'Fictional post-staging alteration')
                elif mutation == 'symlink':
                    path.unlink()
                    path.symlink_to(root / 'LICENSE')
                else:
                    (destination / 'third-party' / mutation).write_bytes(b'Fictional unexpected bytes')
                with self.assertRaises(ValueError):
                    package.verify_notice_stage(destination)

    def test_traversal_errors_abort_for_root_and_unexpected_subdirectories(self):
        for location in ('root', 'unexpected child', 'staging child'):
            for error in (PermissionError('Synthetic unreadable directory'), OSError('Synthetic traversal failure')):
                with self.subTest(location=location, error=type(error)), tempfile.TemporaryDirectory() as tmp, patch.object(package, 'ROOT', Path(tmp)):
                    root = Path(tmp)
                    self.notice_fixture(root)
                    destination = root / 'installer-notices'
                    if location == 'staging child':
                        package.licenses(destination)
                        denied = destination / 'third-party/unexpected'
                    else:
                        denied = root / ('third-party' if location == 'root' else 'third-party/unexpected')
                    denied.mkdir(parents=True, exist_ok=True)
                    (denied / 'fictional.key').write_bytes(b'Synthetic ignored fixture')
                    scandir = package.os.scandir

                    def failing_scandir(path):
                        if Path(path) == denied:
                            raise error
                        return scandir(path)

                    with patch.object(package.os, 'scandir', side_effect=failing_scandir):
                        with self.assertRaises(type(error)):
                            package.licenses(destination)

    def test_nsis_wrapper_checks_stage_before_and_after_compiler(self):
        for timing in ('before', 'during', 'unchanged'):
            with self.subTest(timing=timing), tempfile.TemporaryDirectory() as tmp, patch.object(package, 'ROOT', Path(tmp)):
                root = Path(tmp)
                self.notice_fixture(root)
                destination = root / 'installer-notices'
                package.licenses(destination)
                control = root / 'synthetic.nsh'
                control.write_bytes(b'Fictional control bytes')
                manifest = root / 'inputs.json'
                manifest.write_text(json.dumps({str(control): package.digest(control)}))
                target = destination / 'LICENSE'
                if timing == 'before':
                    target.write_bytes(b'Fictional precompile alteration')

                def compiler(*args, **kwargs):
                    if timing == 'during':
                        target.write_bytes(b'Fictional compile-time alteration')

                # Native sharing-lock semantics remain a Windows acceptance item;
                # this exercises the actual wrapper's verification boundaries.
                with patch.object(package, 'locked_windows_notice_stage', return_value=nullcontext()), patch.object(package.subprocess, 'run', side_effect=compiler) as run:
                    if timing == 'unchanged':
                        package.run_verified_notice_command(destination, ['synthetic-nsis'], manifest)
                        run.assert_called_once_with(['synthetic-nsis'], check=True)
                    else:
                        with self.assertRaisesRegex(ValueError, 'Staged notice hash/type mismatch'):
                            package.run_verified_notice_command(destination, ['synthetic-nsis'], manifest)
                        self.assertEqual(run.call_count, 0 if timing == 'before' else 1)

    def test_all_stage_entrypoints_reject_invalid_approval_inventories(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(package, 'ROOT', Path(tmp)):
            root = Path(tmp)
            self.notice_fixture(root)
            stage = root / 'stage'
            package.licenses(stage)
            policy = root / 'packaging/notice-allowlist.json'
            original = json.loads(policy.read_text())
            invalid = [None, [], {}, {'schema': 1, 'files': {}},
                       {**original, 'files': {}}, {**original, 'schema': True},
                       {**original, 'lock_sha256': '0' * 64},
                       {**original, 'files': {k: v for k, v in original['files'].items() if k != 'LICENSE'}}]
            for inventory in invalid:
                with self.subTest(inventory_type=type(inventory).__name__):
                    policy.write_text(json.dumps(inventory))
                    with self.assertRaises((ValueError, TypeError, KeyError)):
                        package.licenses(root / 'new-stage')
                    with self.assertRaises((ValueError, TypeError, KeyError)):
                        package.verify_notice_stage(stage)
                    with patch.object(package.subprocess, 'run') as run:
                        with self.assertRaises((ValueError, TypeError, KeyError)):
                            package.run_verified_notice_command(stage, ['fictional-compiler'], root / 'absent.json')
                        run.assert_not_called()
            policy.write_text(json.dumps(original))
            package.verify_notice_stage(stage)

    def test_destination_links_never_modify_their_targets(self):
        for location in ('LICENSE', 'DejaVu-LICENSE.txt', 'ancestor'):
            with self.subTest(location=location), tempfile.TemporaryDirectory() as tmp, patch.object(package, 'ROOT', Path(tmp)):
                root = Path(tmp)
                self.notice_fixture(root)
                stage = root / 'stage'
                stage.mkdir()
                target = root / 'fictional-protected.txt'
                target.write_bytes(b'Fictional protected sentinel')
                if location == 'ancestor':
                    stage.rmdir()
                    stage.symlink_to(root, target_is_directory=True)
                else:
                    (stage / location).symlink_to(target)
                with self.assertRaises(ValueError):
                    package.licenses(stage)
                self.assertEqual(target.read_bytes(), b'Fictional protected sentinel')

    def test_atomic_copy_rejects_a_destination_link_inserted_during_copy(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / 'source'
            source.write_bytes(b'Fictional approved data')
            target = root / 'sentinel'
            target.write_bytes(b'Fictional protected data')
            destination = root / 'destination'
            fsync = package.os.fsync
            def insert_link(descriptor):
                destination.symlink_to(target)
                return fsync(descriptor)
            with patch.object(package.os, 'fsync', side_effect=insert_link):
                with self.assertRaises(ValueError):
                    package.safe_copy(source, destination, package.digest(source))
            self.assertEqual(target.read_bytes(), b'Fictional protected data')
            self.assertFalse(list(root.glob('.dpts-*')))

    def test_snapshot_and_recording_bind_the_verified_digest(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            candidate = root / 'candidate'
            candidate.write_bytes(b'Fictional verified artifact')
            snapshot = root / 'private/snapshot'
            checksum = package.snapshot_artifact(candidate, snapshot)
            candidate.write_bytes(b'Fictional replaced candidate')
            self.assertEqual(snapshot.read_bytes(), b'Fictional verified artifact')
            destination = root / 'public/artifact'
            with patch.object(package, 'record') as record:
                package.promote_verified(snapshot, destination, 'synthetic-target', 'deb', checksum)
                record.assert_called_once_with(destination, 'synthetic-target', 'deb', checksum)
            self.assertEqual(package.digest(destination), checksum)
            snapshot.chmod(0o600)
            snapshot.write_bytes(b'Fictional corrupted snapshot')
            with patch.object(package, 'record') as record:
                with self.assertRaisesRegex(ValueError, 'changed before promotion'):
                    package.promote_verified(snapshot, destination, 'synthetic-target', 'deb', checksum)
                record.assert_not_called()
            with self.assertRaisesRegex(ValueError, 'changed before recording'):
                package.record(candidate, 'synthetic-target', 'deb', checksum)
            with self.assertRaisesRegex(ValueError, 'requires the verified'):
                package.record(candidate, 'synthetic-target', 'deb')

    def test_tampered_nsis_control_and_executable_inputs_abort(self):
        for filename in ('third-party-install.nsh', 'installer.nsi', 'DirectPaymentTimesheets.exe'):
            with self.subTest(filename=filename), tempfile.TemporaryDirectory() as tmp, patch.object(package, 'ROOT', Path(tmp)):
                root = Path(tmp)
                self.notice_fixture(root)
                stage = root / 'stage'
                package.licenses(stage)
                control = root / filename
                control.write_bytes(b'Fictional approved NSIS input')
                manifest = root / 'inputs.json'
                manifest.write_text(json.dumps({str(control): package.digest(control)}))
                control.write_bytes(b'Fictional corrupted NSIS input')
                with patch.object(package, 'locked_windows_notice_stage', return_value=nullcontext()), patch.object(package.subprocess, 'run') as run:
                    with self.assertRaisesRegex(ValueError, 'NSIS input hash mismatch'):
                        package.run_verified_notice_command(stage, ['fictional-nsis'], manifest)
                    run.assert_not_called()

    def test_windows_extracted_payload_rejects_additions_and_modified_executable(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(package, 'ROOT', Path(tmp)):
            root = Path(tmp)
            self.notice_fixture(root)
            stage = root / 'stage'
            package.licenses(stage)
            exe = stage / 'DirectPaymentTimesheets.exe'
            exe.write_bytes(b'Fictional executable fixture')
            expected = package.staged_notice_inventory(package.approved_inventory())
            expected[exe.name] = package.digest(exe)
            package.verify_windows_payload(stage, expected)
            extra = stage / 'fictional.key'
            extra.write_bytes(b'Fictional unexpected bytes')
            with self.assertRaisesRegex(ValueError, 'Unexpected or missing'):
                package.verify_windows_payload(stage, expected)
            extra.unlink()
            exe.write_bytes(b'Fictional altered executable')
            with self.assertRaisesRegex(ValueError, 'payload hash mismatch'):
                package.verify_windows_payload(stage, expected)

    def windows_fixture(self, root, name='build'):
        binary = root / 'fictional.exe'
        binary.write_bytes(b'Fictional application executable')
        compiler = root / 'toolchain/makensis.exe'
        compiler.parent.mkdir(exist_ok=True)
        compiler.write_bytes(b'Fictional compiler executable')
        plugin = compiler.parent / 'Plugins/x86-unicode/System.dll'
        plugin.parent.mkdir(parents=True, exist_ok=True)
        plugin.write_bytes(b'Fictional NSIS plug-in')
        extractor = root / 'fictional-7z.exe'
        extractor.write_bytes(b'Fictional extractor executable')
        build = root / name
        output = root / 'output/DirectPaymentTimesheets-1.0.7-windows-x86_64-setup.exe'
        provenance = dict(sha256=package.digest(binary), source_sha='a' * 40)
        with patch.object(package, 'binary_provenance', return_value=provenance), patch.object(package, 'validate_pe'), patch.object(package, 'version', return_value='1.0.7'), patch.object(package.subprocess, 'check_output', return_value=b'Fictional committed control source'):
            checksum = package.prepare_windows_build(binary, compiler, extractor, build, output, 'x86_64-pc-windows-msvc')
        return build / 'build.json', checksum

    def test_windows_builds_isolate_controls_and_pin_manifest_bytes(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(package, 'ROOT', Path(tmp)):
            root = Path(tmp)
            self.notice_fixture(root)
            first, checksum = self.windows_fixture(root, 'first')
            second, _ = self.windows_fixture(root, 'second')
            a, b = json.loads(first.read_text()), json.loads(second.read_text())
            self.assertNotEqual(a['stage'], b['stage'])
            for name in ('third-party-install.nsh', 'third-party-uninstall.nsh', 'inputs/installer.nsi', 'inputs/DirectPaymentTimesheets.exe'):
                self.assertIn(str(first.parent / name), a['files'])
                self.assertIn(str(second.parent / name), b['files'])
                self.assertNotEqual(first.parent / name, second.parent / name)
            first.write_text(first.read_text() + ' ')
            with patch.object(package.subprocess, 'run') as run:
                with self.assertRaisesRegex(ValueError, 'manifest changed'):
                    package.compile_windows_build(first, checksum)
                run.assert_not_called()

    def test_windows_pipeline_rejects_tampering_and_checks_real_extracted_bytes(self):
        for mutation in ('control', 'payload', 'snapshot', 'candidate', 'missing marker', 'duplicate marker', 'manifest reopen', 'unchanged'):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as tmp, patch.object(package, 'ROOT', Path(tmp)):
                root = Path(tmp)
                self.notice_fixture(root)
                manifest, checksum = self.windows_fixture(root)
                spec = json.loads(manifest.read_text())
                if mutation == 'control':
                    (manifest.parent / 'third-party-install.nsh').write_bytes(b'Fictional injected File directive')
                uninstaller = b'Fictional generated uninstaller'
                generated = hashlib.sha256(uninstaller).hexdigest()

                def tools(command, **kwargs):
                    if command == spec['command']:
                        candidate_bytes = b'Fictional installer container'
                        Path(spec['artifact']).write_bytes(b'Fictional replaced candidate' if mutation == 'candidate' else candidate_bytes)
                        installer_hash = hashlib.sha256(candidate_bytes).hexdigest()
                        output = 'DPTS_UNINSTALLER_SHA256:' + generated + '\n'
                        if mutation != 'missing marker':
                            output += 'DPTS_INSTALLER_SHA256:' + installer_hash + '\n'
                        if mutation == 'duplicate marker':
                            output += 'DPTS_INSTALLER_SHA256:' + installer_hash + '\n'
                        return type('Result', (), {'stdout': output})()
                    extracted = manifest.parent / 'extracted'
                    package.licenses(extracted)
                    for name in ('DirectPaymentTimesheets.exe', 'direct-payment-timesheets.ico'):
                        data = (manifest.parent / 'inputs' / name).read_bytes()
                        (extracted / name).write_bytes(data)
                    plugin = extracted / '$PLUGINSDIR/System.dll'
                    plugin.parent.mkdir()
                    plugin.write_bytes((root / 'toolchain/Plugins/x86-unicode/System.dll').read_bytes())
                    (extracted / 'Uninstall.exe').write_bytes(uninstaller)
                    if mutation == 'payload':
                        (extracted / 'DirectPaymentTimesheets.exe').write_bytes(b'Fictional wrong payload')
                    elif mutation == 'snapshot':
                        snapshot = Path(command[-1])
                        snapshot.chmod(0o600)
                        snapshot.write_bytes(b'Fictional replaced installer snapshot')
                    return type('Result', (), {'stdout': ''})()

                original_read = Path.read_text
                def altered_reopen(path, *args, **kwargs):
                    if mutation == 'manifest reopen' and path == manifest:
                        return '{"schema":1,"files":{},"command":["fictional-unapproved-command"]}'
                    return original_read(path, *args, **kwargs)

                with patch.object(Path, 'read_text', altered_reopen), patch.object(package, 'locked_windows_notice_stage', return_value=nullcontext()) as locks, patch.object(package, 'locked_windows_paths', return_value=nullcontext()), patch.object(package.subprocess, 'run', side_effect=tools) as run, patch.object(package, 'record') as record:
                    if mutation in ('unchanged', 'manifest reopen'):
                        package.compile_windows_build(manifest, checksum)
                        record.assert_called_once()
                        output = Path(spec['publish'])
                        self.assertEqual(output.read_bytes(), b'Fictional installer container')
                        self.assertEqual(record.call_args.args[-1], package.digest(output))
                    else:
                        with self.assertRaises(ValueError):
                            package.compile_windows_build(manifest, checksum)
                        record.assert_not_called()
                        self.assertFalse(Path(spec['publish']).exists())
                        if mutation == 'control':
                            run.assert_not_called()
                    self.assertIn(str(manifest.parent / 'third-party-install.nsh'), locks.call_args.args[2])
                    self.assertIn(str(manifest.parent / 'inputs/DirectPaymentTimesheets.exe'), locks.call_args.args[2])

    def test_committed_source_export_ignores_working_tree_replacement(self):
        stream = io.BytesIO()
        with tarfile.open(fileobj=stream, mode='w') as tar:
            data = b'Fictional committed source'
            member = tarfile.TarInfo('fictional.rs')
            member.size = len(data)
            member.mode = 0o755
            tar.addfile(member, io.BytesIO(data))
        class GitArchive:
            def __enter__(self):
                self.stdout = io.BytesIO(stream.getvalue())
                return self
            def __exit__(self, *args):
                self.stdout.close()
            def wait(self):
                return 0
        with tempfile.TemporaryDirectory() as tmp, patch.object(source_bundle, 'ROOT', Path(tmp)):
            (Path(tmp) / 'fictional.rs').write_bytes(b'Fictional working-tree replacement')
            collected = []
            oid = hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest()
            tree = ('100755 blob ' + oid + '\tfictional.rs\0').encode()
            with patch.object(source_bundle.subprocess, 'Popen', return_value=GitArchive()) as git, patch.object(source_bundle.subprocess, 'check_output', return_value=tree):
                source_bundle.add_committed_sources('a' * 40, lambda *args: collected.append(args))
            self.assertEqual(collected, [('application/fictional.rs', b'Fictional committed source', True)])
            self.assertEqual(git.call_args.args[0][-1], 'a' * 40)
            for invalid_tree in [tree.replace(oid.encode(), b'0' * 40),
                                 tree + b'100644 blob ' + b'0' * 40 + b'\tmissing.rs\0']:
                with patch.object(source_bundle.subprocess, 'Popen', return_value=GitArchive()), patch.object(source_bundle.subprocess, 'check_output', return_value=invalid_tree):
                    with self.assertRaises(ValueError):
                        source_bundle.add_committed_sources('a' * 40, lambda *args: None)


    def test_verifiers_do_not_reopen_a_changed_archive_path(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            archive = root / 'fixture-1.0.0.crate'
            with tarfile.open(archive, 'w:gz') as tar:
                member = tarfile.TarInfo('fixture-1.0.0/src/lib.rs')
                data = b'// Fictional checksum-verified source'
                member.size = len(data)
                tar.addfile(member, io.BytesIO(data))
            checksum = hashlib.sha256(archive.read_bytes()).hexdigest()
            manifest = dict(crates=[dict(name='fixture', version='1.0.0', sha256=checksum)])
            original_open = vendor.tarfile.open
            first = True
            def mutate_archive(*args, **kwargs):
                nonlocal first
                if first:
                    first = False
                    archive.write_bytes(b'Fictional unverified archive replacement')
                return original_open(*args, **kwargs)
            with patch.object(vendor.tarfile, 'open', side_effect=mutate_archive):
                vendor.prepare(manifest, root, root / 'vendor')
            self.assertEqual((root / 'vendor/fixture-1.0.0/src/lib.rs').read_bytes(), data)
            import third_party_licenses
            p = dict(name='fixture', version='1.0.0', checksum=checksum)
            with patch.object(third_party_licenses, 'locked_archives', return_value=iter([(p, archive)])):
                with self.assertRaisesRegex(ValueError, 'changed after verification'):
                    generate(root)

    def test_output_promotion_rejects_concurrent_builds(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / 'verified'
            source.write_bytes(b'Fictional verified artifact')
            output = root / 'output/artifact'
            with package.artifact_output_lock(output), patch.object(package, 'record') as record:
                with self.assertRaises(FileExistsError):
                    package.promote_verified(source, output, 'fictional-target', 'deb', package.digest(source))
                record.assert_not_called()
                self.assertFalse(output.exists())
            with patch.object(package, 'record'):
                package.promote_verified(source, output, 'fictional-target', 'deb', package.digest(source))
            self.assertFalse(list(output.parent.glob('*.lock')))

    def test_bound_provenance_and_symlink_sidecars(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(package, 'ROOT', Path(tmp)), patch.object(package, 'source_sha', return_value='a' * 40), patch.object(package, 'version', return_value='1.0.7'):
            root = Path(tmp)
            (root / 'packaging').mkdir()
            (root / 'packaging/toolchain.json').write_text(json.dumps({'rust': 'fictional-toolchain'}))
            artifact = root / 'direct-payment-timesheets_1.0.7_amd64.deb'
            artifact.write_bytes(b'Fictional verified package')
            checksum = package.digest(artifact)
            package.record(artifact, 'x86_64-unknown-linux-gnu', 'deb', checksum)
            metadata = artifact.with_name(artifact.name + '.json')
            self.assertEqual(json.loads(metadata.read_text())['sha256'], checksum)
            metadata.unlink()
            sentinel = root / 'sentinel'
            sentinel.write_bytes(b'Fictional protected sentinel')
            metadata.symlink_to(sentinel)
            with self.assertRaises(ValueError):
                package.record(artifact, 'x86_64-unknown-linux-gnu', 'deb', checksum)
            self.assertEqual(sentinel.read_bytes(), b'Fictional protected sentinel')

    def test_offline_vendor_preserves_checksums_and_refuses_traversal(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for bad in (False, True):
                archive = root / 'fixture-1.0.0.crate'
                with tarfile.open(archive, 'w:gz') as tar:
                    info = tarfile.TarInfo('fixture-1.0.0/' + ('../escape' if bad else 'src/lib.rs'))
                    data = b'// Synthetic source\n'
                    info.size = len(data)
                    tar.addfile(info, io.BytesIO(data))
                checksum = hashlib.sha256(archive.read_bytes()).hexdigest()
                manifest = dict(crates=[dict(name='fixture', version='1.0.0', sha256=checksum)])
                destination = root / ('unsafe' if bad else 'vendor')
                if bad:
                    with self.assertRaisesRegex(ValueError, 'Unsafe source member'):
                        vendor.prepare(manifest, root, destination)
                    self.assertFalse(destination.exists())
                    self.assertFalse((root / 'escape').exists())
                else:
                    vendor.prepare(manifest, root, destination)
                    metadata = json.loads((destination / 'fixture-1.0.0/.cargo-checksum.json').read_text())
                    self.assertEqual(metadata['package'], checksum)
                    self.assertEqual(metadata['files']['src/lib.rs'], hashlib.sha256(data).hexdigest())

    def test_vendor_rejects_windows_style_archive_traversal_on_every_host(self):
        for member_name in ('..\\escape', 'C:escape'):
            with self.subTest(member=member_name), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                archive = root / 'fixture-1.0.0.crate'
                with tarfile.open(archive, 'w:gz') as tar:
                    member = tarfile.TarInfo('fixture-1.0.0/' + member_name)
                    member.size = 1
                    tar.addfile(member, io.BytesIO(b'x'))
                manifest = dict(crates=[dict(name='fixture', version='1.0.0', sha256=package.digest(archive))])
                with self.assertRaisesRegex(ValueError, 'Unsafe source member'):
                    vendor.prepare(manifest, root, root / 'vendor')
                self.assertFalse((root / 'vendor').exists())

    def test_rustsec_ranges_keep_prereleases_and_zero_major_bounds(self):
        for value, requirement, expected in [
            ('0.23.44', '>=0.23.46', False),
            ('0.23.46', '>=0.23.46', True),
            ('0.103.17', '>=0.103.16, <0.104.0-alpha.1', True),
            ('0.104.0-alpha.2', '>=0.103.16, <0.104.0-alpha.1', False),
            ('1.54.0-beta.1', '>=1.53.3', False),
            ('7.4.1', '^7.4.0', True), ('8.0.0', '^7.4.0', False),
            ('0.13.5', '^0.13.4', True), ('0.14.0', '^0.13.4', False),
            ('111.9.1', '>=111.9', True), ('111.8.99', '>=111.9', False),
            ('1.0.99', '<=1.0', True), ('1.1.0', '>1.0', True),
            ('0.0.9', '^0.0', True), ('0.1.0', '^0.0', False),
        ]:
            with self.subTest(value=value, requirement=requirement):
                self.assertEqual(matches(value, requirement), expected)
        with self.assertRaises(ValueError):
            matches('1.0.0', 'unrecognised syntax')

    def test_dirty_and_untracked_source_cannot_be_stamped_as_head(self):
        for state in [' M src/main.rs\n', '?? private-unstaged-name\n']:
            with patch.object(package.subprocess, 'check_output', return_value=state) as git:
                with self.assertRaisesRegex(ValueError, 'exact clean committed source') as error:
                    package.source_sha()
                self.assertNotIn('private-unstaged-name', str(error.exception))
                git.assert_called_once()

    def test_native_notices_are_read_from_verified_archive_not_wrapper_expression(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            cache = root / 'cargo/registry/cache/synthetic'
            cache.mkdir(parents=True)
            archive = cache / 'fixture-1.0.0.crate'
            with tarfile.open(archive, 'w:gz') as tar:
                for name, data in [('Cargo.toml', b'[package]\nname="fixture"\nversion="1.0.0"\nlicense="MIT"\n'),
                                   ('vendor/native/COPYING', b'Native licence differs from the wrapper\n')]:
                    info = tarfile.TarInfo('fixture-1.0.0/' + name)
                    info.size = len(data)
                    tar.addfile(info, io.BytesIO(data))
            checksum = hashlib.sha256(archive.read_bytes()).hexdigest()
            (root / 'Cargo.lock').write_text('[[package]]\nname="fixture"\nversion="1.0.0"\nsource="registry+https://github.com/rust-lang/crates.io-index"\nchecksum="' + checksum + '"\n')
            for name in (*LEGACY_NOTICES, 'LICENSE', 'assets/fonts/DejaVu-LICENSE.txt'):
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(b'Synthetic attribution')
            output = generate(root, root / 'cargo')
            self.assertEqual(output[Path('third-party/crates/fixture-1.0.0/vendor/native/COPYING')],
                             b'Native licence differs from the wrapper\n')
            manifest = json.loads(output[Path('third-party/crates/manifest.json')])
            self.assertEqual(manifest['packages'][0]['archive_sha256'], checksum)
            archive.write_bytes(b'tampered')
            with self.assertRaisesRegex(ValueError, 'checksum-invalid'):
                generate(root, root / 'cargo')


if __name__ == '__main__':
    unittest.main()
