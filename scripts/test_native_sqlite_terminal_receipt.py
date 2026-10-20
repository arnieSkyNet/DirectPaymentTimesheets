"""Synthetic metadata/files and mocked Git/supervisors only; no native children."""
import copy
import contextlib
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import Mock, patch

import native_sqlite_terminal_receipt as r
import native_sqlite_terminal_checks as n


def sample():
    return r.new(dict(profile='github', run_id='12345', attempt=1, S='a'*40, W='b'*40, L='c'*40,
                      architecture='macos-arm64', python='3.12.8', rust='1.98.1', retention_days=90),
                 {name: 'd'*64 for name in ('fixture_source', 'source_inventory', 'controller_inventory', 'manifest', 'executable')})


def passing():
    data = sample()
    for name, case in data['cases'].items():
        case.update(state='PASS', exit_code=0 if name in ('normal', 'retry', 'unwind') else 86,
                    source_verified=True, storage_closed=True)
        case['cleanup'].update(leader_reaped=True, namespace_empty=True, supervisor_handles_closed=True)
        case['phases'] = {key: False for key in r.PHASES}
        case['phases'].update(privacy_ready=True, handles_ready=True)
        if name in ('normal', 'retry', 'unwind'):
            case['phases'].update(completed=True, guards_dropped=True, staging_released=True)
            case['phases']['busy_confirmed'] = name == 'retry'
        elif name == 'stall':
            case.update(exit_code=-9, timed_out=True)
            case['phases']['stall_ready'] = True
        else:
            case['phases'].update(busy_confirmed=True, terminal_ready=True)
            case['phases']['panic_ready'] = name == 'panic_terminal'
        if name in ('rollback', 'wal'):
            case['phases']['pending_ready'] = True
            case['phases']['wal_committed_only'] = name == 'wal'
            rec = case['recovery']
            rec.update(attempted=True, exit_code=0, privacy_ready=True, confirmed=True, integrity_ok=True,
                       rows='BASELINE' if name == 'rollback' else 'WAL_COMMITTED')
            rec['cleanup'].update(leader_reaped=True, namespace_empty=True, supervisor_handles_closed=True)
            witness = dict(sha256='e'*64, size=4096)
            rec['pre'] = {'database': witness.copy(), 'journal' if name == 'rollback' else 'wal': witness.copy()}
            rec['post'] = {'database': dict(sha256='f'*64, size=8192)}
    data['final'].update(state='PASS', driver_exit=0, source_verified=True, storage_closed=True, complete=True)
    return data


class PositiveSchema(unittest.TestCase):
    def test_canonical_round_trip_digest_and_summary_have_only_valid_metadata(self):
        data = passing()
        raw = r.canonical(data)
        self.assertEqual(r.parse(raw), data)
        self.assertEqual(r.canonical(dict(reversed(list(data.items())))), raw)
        summary = r.summary(data)
        self.assertIn(hashlib.sha256(raw).hexdigest(), summary)
        self.assertIn(raw.decode().strip(), summary)
        self.assertEqual(summary.count('| PASS |'), 8)
        self.assertLess(len(raw), 65536)

    def test_local_profile_and_90_day_policy(self):
        data = sample()
        data['run'].update(profile='molly-local', run_id='0', L=None, architecture='windows-x86_64')
        r.validate(data)
        for days in (89, True, '90', 3651):
            data['run']['retention_days'] = days
            with self.assertRaises(r.ReceiptRefusal):
                r.validate(data)

    def test_reject_unknown_fields_at_every_object_boundary(self):
        data = passing()
        objects = [data, data['run'], data['hashes'], data['cases'], data['cases']['wal'],
                   data['cases']['wal']['phases'], data['cases']['wal']['cleanup'],
                   data['cases']['wal']['recovery'], data['cases']['wal']['recovery']['cleanup'],
                   data['cases']['wal']['recovery']['pre'], data['cases']['wal']['recovery']['pre']['database'], data['final']]
        for obj in objects:
            obj['raw_error'] = '/Users/synthetic/SMTP_PASSWORD=synthetic'
            with self.assertRaises(r.ReceiptRefusal):
                r.canonical(data)
            del obj['raw_error']

    def test_reject_missing_cases_paths_users_logs_contents_and_wrong_scalar_types(self):
        mutations = [lambda d: d['cases'].pop('stall'),
                     lambda d: d['run'].update(run_id='/Users/synthetic'),
                     lambda d: d['run'].update(python='3.12.8 username'),
                     lambda d: d['run'].update(S='payroll.sqlite'),
                     lambda d: d['hashes'].update(executable='synthetic database contents'),
                     lambda d: d['cases']['normal'].update(exit_code=True),
                     lambda d: d['cases']['normal']['phases'].update(privacy_ready='raw log'),
                     lambda d: d['cases']['normal'].update(state='failure: user=synthetic'),
                     lambda d: d['run'].update(attempt=2),
                     lambda d: d['run'].update(L=None)]
        for mutate in mutations:
            data = passing()
            mutate(data)
            with self.assertRaises(r.ReceiptRefusal):
                r.canonical(data)

    def test_reject_duplicate_keys_nan_noncanonical_bytes_and_oversize_input(self):
        for raw in (b'{"schema":1,"schema":2}', b'{"schema":NaN}', b'x'*65537, b'null', b'[]', b'\xff'):
            with self.assertRaises(r.ReceiptRefusal):
                r.parse(raw)

    def test_pass_requires_every_final_guard_and_every_case(self):
        for field in ('source_verified', 'storage_closed', 'complete'):
            data = passing()
            data['final'][field] = False
            with self.assertRaises(r.ReceiptRefusal):
                r.validate(data)
        for name in r.CASES:
            for field in ('source_verified', 'storage_closed'):
                data = passing()
                data['cases'][name][field] = False
                with self.assertRaises(r.ReceiptRefusal):
                    r.validate(data)
            data = passing()
            data['cases'][name] = r.empty_case()
            with self.assertRaises(r.ReceiptRefusal):
                r.validate(data)

    def test_wal_recovery_exit_witness_and_cleanup_cannot_be_inferred_from_primary(self):
        for field, value in (('exit_code', 86), ('rows', 'BASELINE'), ('confirmed', False), ('attempted', False)):
            data = passing()
            data['cases']['wal']['recovery'][field] = value
            with self.assertRaises(r.ReceiptRefusal):
                r.validate(data)
        for key in r.CLEANUP[:3]:
            data = passing()
            data['cases']['wal']['recovery']['cleanup'][key] = None
            with self.assertRaises(r.ReceiptRefusal):
                r.validate(data)

    def test_uncertain_cleanup_cannot_be_fail_or_success(self):
        data = sample()
        data['cases']['wal']['state'] = 'UNVERIFIED'
        data['final'].update(state='UNVERIFIED', driver_exit=1)
        r.validate(data)
        for state in ('FAIL', 'PASS'):
            data['cases']['wal']['state'] = state
            with self.assertRaises(r.ReceiptRefusal):
                r.validate(data)


class PrivateOutput(unittest.TestCase):
    def test_local_canonical_copy_checksum_and_offline_verification(self):
        data = passing()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            root.chmod(0o700)
            path = root / 'receipt.json'
            with patch.object(r.os, 'geteuid', return_value=root.stat().st_uid):
                checksum = r.write_local(data, path)
                with self.assertRaises(FileExistsError):
                    r.write_local(data, path)
            self.assertEqual(path.read_bytes(), r.canonical(data))
            self.assertEqual(path.with_suffix('.sha256').read_text(), checksum+'\n')
            self.assertEqual(path.stat().st_mode & 0o777, 0o600)
            with patch('builtins.print'):
                self.assertEqual(r.main(['--verify', str(path)]), 0)
            path.write_text(json.dumps(data, indent=2))
            with self.assertRaises(r.ReceiptRefusal):
                r.main(['--verify', str(path)])

    def test_broad_parent_symlink_and_existing_destination_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            root.chmod(0o755)
            with patch.object(r.os, 'geteuid', return_value=root.stat().st_uid), self.assertRaises(r.ReceiptRefusal):
                r.write_local(passing(), root/'receipt.json')
            root.chmod(0o700)
            (root/'receipt.json').symlink_to(root/'other')
            with patch.object(r.os, 'geteuid', return_value=root.stat().st_uid), self.assertRaises(FileExistsError):
                r.write_local(passing(), root/'receipt.json')
            self.assertFalse((root/'other').exists())

    def test_publish_closes_metadata_storage_before_writing_and_never_reads_raw_fixture(self):
        import native_sqlite_fixture_storage as storage
        owner = Mock(path=Path('/synthetic-private-metadata'))
        order = []
        owner.close.side_effect = lambda: order.append('closed')
        def write(data, path):
            order.append('write')
        with patch.object(storage, 'PrivateRoot', return_value=owner), patch.object(r, 'write_local', side_effect=write), patch.dict(os.environ, {'GITHUB_STEP_SUMMARY':'synthetic'}, clear=True), patch('builtins.open', unittest.mock.mock_open()) as output, patch('builtins.print') as printed:
            r.publish(passing())
        self.assertEqual(order, ['closed', 'write'])
        self.assertTrue(printed.call_args.args[0].startswith('native.sqlite.receipt={'))
        output.assert_called_once_with('synthetic', 'a', encoding='utf-8')

    def test_close_failure_or_invalid_payload_emits_no_success(self):
        import native_sqlite_fixture_storage as storage
        owner = Mock()
        owner.close.side_effect = OSError('synthetic')
        with patch.object(storage, 'PrivateRoot', return_value=owner), patch.object(r, 'write_local') as write, patch('builtins.print') as printed:
            with self.assertRaises(OSError):
                r.publish(passing())
            write.assert_not_called()
            printed.assert_not_called()
        data = passing()
        data['password'] = 'synthetic'
        with patch.object(storage, 'PrivateRoot') as create, self.assertRaises(r.ReceiptRefusal):
            r.publish(data)
        create.assert_not_called()


class SplitLayout(unittest.TestCase):
    def test_source_is_exact_S_not_controller_or_dirty_checkout_and_needs_no_fixture_in_main(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory).resolve()
            source, controller, dirty = [base/name for name in ('source-S','controller-W','dirty-local')]
            for root in (source, controller, dirty):
                root.mkdir()
            fixture = source/n.FIXTURE
            fixture.parent.mkdir()
            content = b'synthetic reviewed Rust fixture'
            fixture.write_bytes(content)
            (dirty/'tests').mkdir()
            (dirty/n.FIXTURE).write_bytes(b'dirty unreviewed fixture')
            blob = hashlib.sha1(b'blob '+str(len(content)).encode()+b'\0'+content).hexdigest()
            records = ('100644 '+blob+' 0\t'+n.FIXTURE+'\0').encode()
            def git(args, **kwargs):
                root = Path(args[2])
                if args[3:] == ['rev-parse','HEAD']:
                    return 'a'*40 if root == source else 'b'*40
                if args[3:] == ['ls-files','-s','-z']:
                    return records
                if args[3:] == ['status','--porcelain=v1','--untracked-files=all']:
                    return b''
                raise AssertionError(args)
            with patch.object(n.subprocess, 'check_output', side_effect=git), patch.object(n, 'REVIEWED_FIXTURE_SHA256', hashlib.sha256(content).hexdigest()):
                self.assertEqual(n.verify_source_fixture(source, 'a'*40), fixture)
                self.assertFalse((controller/n.FIXTURE).exists())
                with self.assertRaises(n.Refusal):
                    n.verify_source_fixture(controller, 'a'*40)
                with self.assertRaises(n.Refusal):
                    n.verify_source_fixture(dirty, 'a'*40)
                fixture.write_bytes(b'dirty tracked edit despite same HEAD')
                with self.assertRaises(n.Refusal):
                    n.verify_source_fixture(source, 'a'*40)

    def test_manifest_rejects_old_committed_fixture_even_if_other_inventory_matches(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            (root/'tests').mkdir()
            (root/n.FIXTURE).write_bytes(b'old fixture')
            with patch.object(n.subprocess,'check_output',side_effect=['a'*40, b'100644 '+b'a'*40+b' 0\t'+n.FIXTURE.encode()+b'\0']), self.assertRaises(n.Refusal):
                n.verify_source_fixture(root,'a'*40,{n.FIXTURE:{'sha256':'e'*64}})

class DriverLifecycle(unittest.TestCase):
    def exercise(self, failure=None):
        parent = tempfile.TemporaryDirectory()
        self.addCleanup(parent.cleanup)
        base = Path(parent.name).resolve()
        executable = base/'not-executable'
        executable.write_bytes(b'SYNTHETIC, NEVER EXECUTED')
        data = sample()
        owners, exported, invocations = [], [], []
        def owner():
            root = base/('case-'+str(len(owners)))
            root.mkdir()
            obj = Mock(path=root)
            def close():
                case = data['cases'][r.CASES[len(owners)-1]]
                self.assertTrue(case['source_verified'])
                self.assertNotEqual(case['state'], 'PASS')
                if failure == 'close':
                    raise OSError('never serialize me: synthetic username')
            obj.close.side_effect = close
            owners.append(obj)
            return obj
        def execute(exe, root, name, system, sha):
            invocations.append(name)
            if failure == 'primary-cleanup' and name == 'normal':
                raise n.Refusal('unsupported_cleanup')
            for marker in ('go','privacy_ready','completed','recovery_confirmed'):
                (root/marker).unlink(missing_ok=True)
            if name.startswith('recover_'):
                if failure == 'recovery-cleanup':
                    raise n.Refusal('unsupported_cleanup')
                if failure == 'recovery-timeout':
                    error=subprocess.TimeoutExpired('synthetic',3)
                    error.cleanup_receipt=dict(final_status=-9,reaped=True,descendants_gone=True,handles_closed=True)
                    raise error
                (root/'privacy_ready').touch()
                (root/'recovery_confirmed').touch()
                (root/'stage/synthetic.sqlite').write_bytes(b'synthetic recovered image')
                return 0
            prototype = passing()['cases'][name]
            for marker, present in prototype['phases'].items():
                if present:
                    (root/marker).touch()
            if name in ('rollback','wal'):
                (root/'stage').mkdir()
                (root/'stage/synthetic.sqlite').write_bytes(b'synthetic preimage')
                (root/('stage/synthetic.sqlite'+('-wal' if name=='wal' else '-journal'))).write_bytes(b'synthetic sidecar')
            if name == 'stall':
                error = subprocess.TimeoutExpired('synthetic',3)
                error.cleanup_receipt = dict(final_status=-9,reaped=True,descendants_gone=True,handles_closed=True)
                raise error
            return prototype['exit_code']
        checks = [0]
        def admitted(sha):
            checks[0] += 1
            if failure == 'source' and checks[0] == 3:
                raise n.Refusal('unapproved_source')
            return executable
        def publish(receipt):
            r.validate(receipt)
            exported.append(copy.deepcopy(receipt))
        with contextlib.ExitStack() as stack:
            stack.enter_context(patch.object(n.sys,'argv',['checks','--run']))
            stack.enter_context(patch.object(n.subprocess,'check_output',side_effect=['a'*40,'']))
            stack.enter_context(patch.object(n.platform,'system',return_value='Darwin'))
            stack.enter_context(patch.object(n.platform,'machine',return_value='arm64'))
            stack.enter_context(patch.object(n,'prerequisites'))
            stack.enter_context(patch.object(n,'approved_executable',side_effect=admitted))
            stack.enter_context(patch.object(n,'new_sanitized_receipt',return_value=data))
            preflight = stack.enter_context(patch.object(n,'terminal_preflight'))
            stack.enter_context(patch.object(n,'PrivateRoot',side_effect=owner))
            stack.enter_context(patch.object(n,'execute',side_effect=execute))
            stack.enter_context(patch.object(n,'recovery_material'))
            stack.enter_context(patch.object(n.os,'umask'))
            stack.enter_context(patch.object(r,'publish',side_effect=publish))
            deleted = stack.enter_context(patch.object(n.shutil,'rmtree'))
            stack.enter_context(patch('builtins.print'))
            if failure:
                with self.assertRaises((n.Refusal,OSError)):
                    n.main()
            else:
                self.assertEqual(n.main(),0)
            deleted.assert_not_called()
            preflight.assert_called_once_with('Darwin')
        self.assertEqual(len(exported),1)
        return exported[0],owners,invocations

    def test_all_eight_results_preserve_primary_phases_and_separate_recovery_exits(self):
        data,owners,calls = self.exercise()
        self.assertEqual(data['final']['state'],'PASS')
        self.assertEqual(len(calls),10)
        self.assertEqual(len(owners),8)
        for case in ('rollback','wal'):
            record = data['cases'][case]
            self.assertTrue(record['phases']['terminal_ready'])
            self.assertFalse(record['phases']['completed'])
            self.assertEqual(record['exit_code'],86)
            self.assertEqual(record['recovery']['exit_code'],0)
            self.assertTrue(record['recovery']['confirmed'])
            self.assertNotEqual(record['recovery']['pre']['database'],record['recovery']['post']['database'])
            self.assertTrue((owners[r.CASES.index(case)].path/'pre-recovery/synthetic.sqlite').exists())
        for owner in owners:
            owner.close.assert_called_once()

    def test_final_source_failure_cannot_record_success_or_close_storage(self):
        data,owners,calls = self.exercise('source')
        self.assertEqual(data['final']['state'],'FAIL')
        self.assertEqual(data['cases']['normal']['state'],'FAIL')
        self.assertFalse(data['cases']['normal']['source_verified'])
        owners[0].close.assert_not_called()
        self.assertEqual(calls,['normal'])

    def test_storage_close_failure_cannot_record_success(self):
        data,owners,calls = self.exercise('close')
        self.assertEqual(data['cases']['normal']['state'],'UNVERIFIED')
        self.assertEqual(data['final']['state'],'UNVERIFIED')
        self.assertFalse(data['cases']['normal']['storage_closed'])
        self.assertEqual(calls,['normal'])

    def test_primary_cleanup_uncertainty_exports_only_unknown_metadata_and_stops(self):
        data,owners,calls = self.exercise('primary-cleanup')
        self.assertEqual(data['final']['state'],'UNVERIFIED')
        self.assertTrue(all(v is None for v in data['cases']['normal']['phases'].values()))
        owners[0].verify.assert_not_called()
        owners[0].close.assert_not_called()
        self.assertEqual(calls,['normal'])

    def test_recovery_timeout_records_actual_kill_exit_separately_and_stops(self):
        data,owners,calls=self.exercise('recovery-timeout')
        record=data['cases']['rollback']
        self.assertEqual(record['state'],'FAIL')
        self.assertEqual(record['exit_code'],86)
        self.assertEqual(record['recovery']['exit_code'],-9)
        self.assertTrue(record['recovery']['timed_out'])
        self.assertTrue(record['recovery']['cleanup']['leader_reaped'])
        self.assertEqual(calls[-1],'recover_rollback')
        owners[-1].close.assert_not_called()

    def test_recovery_cleanup_uncertainty_keeps_primary_proof_and_never_continues(self):
        data,owners,calls = self.exercise('recovery-cleanup')
        case=data['cases']['rollback']
        self.assertEqual(case['state'],'UNVERIFIED')
        self.assertEqual(data['final']['state'],'UNVERIFIED')
        self.assertEqual(case['exit_code'],86)
        self.assertTrue(case['cleanup']['leader_reaped'])
        self.assertIsNone(case['recovery']['cleanup']['leader_reaped'])
        self.assertEqual(calls[-1],'recover_rollback')
        self.assertNotIn('wal',calls)
        owners[-1].close.assert_not_called()

class ControllerLauncherLayout(unittest.TestCase):
    def test_controller_is_verified_at_W_while_launcher_is_at_L_without_main_rust_fixture(self):
        import yaml
        with tempfile.TemporaryDirectory() as directory:
            base=Path(directory).resolve()
            source, controller, launcher=[base/name for name in ('S','W','L')]
            for root in (source,controller,launcher):
                root.mkdir()
            fixture=b'synthetic pinned native fixture'
            (source/'tests').mkdir()
            (source/n.FIXTURE).write_bytes(fixture)
            (controller/'scripts').mkdir()
            script=controller/'scripts/native_sqlite_terminal_checks.py'
            script.write_bytes(b'# synthetic immutable controller')
            workflow=(Path(__file__).resolve().parents[1]/'.github/workflows/sqlite-terminal-native.yml').read_text()
            doc=yaml.load(workflow,Loader=yaml.BaseLoader)
            job=doc['jobs']['native']
            job['env']['EXPECTED_SHA']='a'*40
            job['env']['EXPECTED_CONTROLLER_SHA']='b'*40
            for step in job['steps']:
                if step.get('uses','').startswith('actions/checkout@'):
                    step['with']['ref']='a'*40 if step['with']['path']=='source' else 'b'*40
            (launcher/'.github/workflows').mkdir(parents=True)
            launchfile=launcher/'.github/workflows/sqlite-terminal-native.yml'
            launchfile.write_text(yaml.dump(doc))
            pinned={root:{p.relative_to(root).as_posix():p.read_bytes() for p in root.rglob('*') if p.is_file()} for root in (source,controller,launcher)}
            def git(args,**kwargs):
                root=Path(args[2])
                if args[3:]==['rev-parse','HEAD']:
                    return {source:'a'*40,controller:'b'*40,launcher:'c'*40}[root]
                if args[3:]==['status','--porcelain=v1','--untracked-files=all']:
                    return b''
                if args[3:]==['ls-files','-s','-z']:
                    entries=[]
                    for name,content in pinned[root].items():
                        blob=hashlib.sha1(b'blob '+str(len(content)).encode()+b'\0'+content).hexdigest()
                        entries.append(('100644 '+blob+' 0\t'+name+'\0').encode())
                    return b''.join(entries)
                raise AssertionError(args)
            with patch.dict(os.environ,{'EXPECTED_CONTROLLER_SHA':'b'*40},clear=True),patch.object(n.subprocess,'check_output',side_effect=git),patch.object(n,'REVIEWED_FIXTURE_SHA256',hashlib.sha256(fixture).hexdigest()):
                n.verify_source_fixture(source,'a'*40)
                n.verify_controller(controller)
                inventory=n.source_inventory(launcher)
                self.assertEqual(inventory['.github/workflows/sqlite-terminal-native.yml']['sha256'],hashlib.sha256(launchfile.read_bytes()).hexdigest())
                self.assertFalse((controller/n.FIXTURE).exists())
                self.assertFalse((launcher/n.FIXTURE).exists())
                self.assertEqual(job['env']['DPT_NATIVE_SQLITE_LAUNCHER_SHA'],'${{ github.workflow_sha }}')
                self.assertEqual({x['with']['path']:x['with']['ref'] for x in job['steps'] if x.get('uses','').startswith('actions/checkout@')},{'source':'a'*40,'controller':'b'*40})
                with self.assertRaises(n.Refusal):
                    n.verify_controller(launcher)
                script.write_bytes(b'# dirty controller hidden by Git flags')
                with self.assertRaises(n.Refusal):
                    n.verify_controller(controller)

class RetentionAndDelivery(unittest.TestCase):
    def test_retention_copies_only_verified_canonical_metadata_and_checksum(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory).resolve()
            root.chmod(0o700)
            first=root/'original.json'
            with patch.object(r.os,'geteuid',return_value=root.stat().st_uid),patch('builtins.print'):
                r.write_local(passing(),first)
                self.assertEqual(r.main(['--retain',str(first),str(root/'retained.json')]),0)
            self.assertEqual(first.read_bytes(),(root/'retained.json').read_bytes())
            self.assertEqual(first.with_suffix('.sha256').read_bytes(),(root/'retained.sha256').read_bytes())
            self.assertEqual(set(p.name for p in root.iterdir()),{'original.json','original.sha256','retained.json','retained.sha256'})

    def test_symlink_checksum_is_refused_without_reading_target(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory).resolve()
            path=root/'receipt.json'
            path.write_bytes(r.canonical(passing()))
            (root/'receipt.sha256').symlink_to(root/'missing-sensitive-target')
            from native_sqlite_fixture_storage import StorageRefusal
            with self.assertRaises(StorageRefusal):
                r.main(['--verify',str(path)])

    def test_summary_delivery_failure_does_not_emit_canonical_success_to_stdout(self):
        import native_sqlite_fixture_storage as storage
        with patch.object(storage,'PrivateRoot',return_value=Mock(path=Path('/synthetic'))),patch.object(r,'write_local'),patch.dict(os.environ,{'GITHUB_STEP_SUMMARY':'synthetic'},clear=True),patch('builtins.open',side_effect=OSError('synthetic private details')),patch('builtins.print') as printed:
            with self.assertRaises(OSError):
                r.publish(passing())
            printed.assert_not_called()


class NativeMetadata(unittest.TestCase):
    def test_windows_receipt_requires_exclusive_handle_release_and_actual_stall_exit(self):
        data=passing()
        data['run'].update(profile='molly-local',run_id='123',L=None,architecture='windows-x86_64')
        data['cases']['stall']['exit_code']=1
        for case in data['cases'].values():
            case['cleanup']['exclusive_released']=True
            if case['recovery']['attempted']:
                case['recovery']['cleanup']['exclusive_released']=True
        r.validate(data)
        data['cases']['wal']['recovery']['cleanup']['exclusive_released']=None
        with self.assertRaises(r.ReceiptRefusal):
            r.validate(data)

    def test_run_identity_records_actual_tool_version_and_separate_S_W_L(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory).resolve()
            exe=root/'synthetic'
            exe.write_bytes(b'NOT EXECUTABLE')
            manifest=root/'manifest.json'
            inventory={n.FIXTURE:dict(sha256=n.REVIEWED_FIXTURE_SHA256,mode='100644')}
            manifest.write_text(json.dumps(dict(source=inventory)))
            env=dict(DPT_NATIVE_SQLITE_MANIFEST=str(manifest),GITHUB_RUN_ID='123',GITHUB_RUN_ATTEMPT='1',EXPECTED_CONTROLLER_SHA='b'*40,DPT_NATIVE_SQLITE_LAUNCHER_SHA='c'*40)
            with patch.dict(os.environ,env,clear=True),patch.object(n.subprocess,'check_output',return_value='rustc 1.98.1 (synthetic 2026-10-01)\n'),patch.object(n.platform,'system',return_value='Darwin'),patch.object(n.platform,'machine',return_value='arm64'),patch.object(n.platform,'python_version',return_value='3.12.8'),patch.object(n,'source_inventory',return_value={'controller':'synthetic inventory'}):
                data=n.new_sanitized_receipt('a'*40,exe)
                self.assertEqual((data['run']['S'],data['run']['W'],data['run']['L']),('a'*40,'b'*40,'c'*40))
                self.assertEqual(data['run']['rust'],'1.98.1')
                self.assertEqual(data['hashes']['fixture_source'],n.REVIEWED_FIXTURE_SHA256)
                with patch.object(n.subprocess,'check_output',return_value='rustc /Users/synthetic/private-details'):
                    with self.assertRaises(n.Refusal):
                        n.new_sanitized_receipt('a'*40,exe)


if __name__ == '__main__':
    unittest.main()
