import importlib.util
from pathlib import Path
import shutil
import struct
import subprocess
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('appimage', ROOT / 'packaging/linux/appimage.py')
appimage = importlib.util.module_from_spec(spec)
spec.loader.exec_module(appimage)


def fixture(arch):
    bits, machine = {'armhf': (1, 40), 'aarch64': (2, 183), 'x86_64': (2, 62)}[arch]
    data = bytearray(256)
    data[:7] = b'\x7fELF' + bytes([bits, 1, 1])
    data[8:11] = b'AI\x02'
    struct.pack_into('<HH', data, 16, 3, machine)
    if bits == 1:
        struct.pack_into('<II', data, 32, 64, 0x05000400)
        struct.pack_into('<HH', data, 46, 40, 2)
        struct.pack_into('<10I', data, 104, 0, 1, 0, 0, 200, 56, 0, 0, 0, 0)
    else:
        struct.pack_into('<Q', data, 40, 64)
        struct.pack_into('<HH', data, 58, 64, 2)
        struct.pack_into('<IIQQQQIIQQ', data, 128, 0, 1, 0, 0, 200, 56, 0, 0, 0, 0)
    data[220:224] = b'hsqs'  # Misleading runtime string must not be used.
    return data


class AppImageTests(unittest.TestCase):
    def test_all_architectures_extract_as_data_never_execute_wrapper(self):
        for arch in ['armhf', 'aarch64', 'x86_64']:
            with self.subTest(arch=arch), tempfile.TemporaryDirectory() as tmp:
                path = Path(tmp) / 'tool'
                path.write_bytes(fixture(arch) + b'hsqs')
                self.assertEqual(appimage.squashfs_offset(path, arch), 256)
                destination = Path(tmp) / 'missing' / 'deploy' / 'squashfs-root'
                self.assertFalse(destination.parent.exists())
                def require_parent(*args, **kwargs):
                    # Model Bookworm, even when the host unsquashfs creates parents.
                    self.assertTrue(destination.parent.is_dir())
                    self.assertFalse(destination.exists())
                with patch.object(appimage.subprocess, 'run', side_effect=require_parent) as run:
                    appimage.extract(path, arch, destination)
                    self.assertEqual(run.call_args.args[0], ['unsquashfs', '-no-progress', '-o', '256', '-d', str(destination), str(path)])
                    self.assertTrue(run.call_args.kwargs['check'])
                for wrong in {'armhf', 'aarch64', 'x86_64'} - {arch}:
                    with self.assertRaises(ValueError): appimage.elf_header(path, wrong)

    def test_rejects_soft_float_malformed_and_missing_squashfs(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'bad'
            original = fixture('armhf') + b'hsqs'
            for offset, replacement in [(36, struct.pack('<I', 0x05000200)), (8, b'\0\0\0'), (46, b'\0\0'), (32, b'\xff'*4), (256, b'nope')]:
                data = original.copy(); data[offset:offset+len(replacement)] = replacement
                path.write_bytes(data)
                with self.assertRaises(ValueError), patch.object(appimage.subprocess, 'run') as run:
                    try: appimage.extract(path, 'armhf', Path(tmp) / 'out')
                    finally: run.assert_not_called()

    def test_qemu_binfmt_rejects_appimage_identifier_not_arm_architecture(self):
        # tonistiigi/binfmt ARM rule matches ELF bytes 8..15 as zero.
        magic = bytes.fromhex('7f454c4601010100000000000000000002002800')
        mask = bytes.fromhex('ffffffffffffff00fffffffffffffffffeffffff')
        header = fixture('armhf')[:20]
        matches = lambda b: all((a & m) == (e & m) for a, e, m in zip(b, magic, mask))
        self.assertFalse(matches(header))
        header[8:11] = bytes(3)
        self.assertTrue(matches(header))

    @unittest.skipUnless(shutil.which('mksquashfs') and shutil.which('unsquashfs'), 'squashfs-tools required')
    def test_real_squashfs_extraction_for_all_architectures_without_emulation(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); source = root / 'source'; source.mkdir()
            (source / 'payload').write_text('verified payload')
            squashfs = root / 'payload.squashfs'
            subprocess.run(['mksquashfs', str(source), str(squashfs), '-noappend', '-processors', '1', '-quiet'], check=True, stdout=subprocess.DEVNULL)
            for arch in ['armhf', 'aarch64', 'x86_64']:
                path = root / arch
                path.write_bytes(fixture(arch) + squashfs.read_bytes())
                for stage in ['deploy', 'image', 'verify']:
                    destination = root / (arch + '-out') / stage / 'squashfs-root'
                    self.assertFalse(destination.parent.exists())
                    appimage.extract(path, arch, destination)
                    self.assertEqual((destination / 'payload').read_text(), 'verified payload')
            # Extraction failures must propagate rather than validate an empty tree.
            path.write_bytes(fixture('x86_64') + b'hsqs')
            with self.assertRaises(subprocess.CalledProcessError):
                appimage.extract(path, 'x86_64', root / 'broken')
