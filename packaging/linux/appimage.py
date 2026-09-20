#!/usr/bin/env python3
"""Inspect/extract type-2 AppImages without executing their runtime (QEMU-safe)."""
from pathlib import Path
import struct
import subprocess
import sys


def elf_header(path, arch):
    with Path(path).open('rb') as stream:
        header = stream.read(64)
    elf_class, machine = {'x86_64': (2, 62), 'armhf': (1, 40), 'aarch64': (2, 183)}[arch]
    if (len(header) < 64 or header[:4] != b'\x7fELF' or
            header[4:7] != bytes([elf_class, 1, 1]) or
            struct.unpack_from('<H', header, 18)[0] != machine):
        raise ValueError(f'{path}: expected little-endian {arch} ELF')
    if arch == 'armhf':
        flags = struct.unpack_from('<I', header, 36)[0]
        if flags & 0xff000000 != 0x05000000 or flags & 0x600 != 0x400:
            raise ValueError(f'{path}: expected ARM EABI5 hard-float ABI')
    return header


def squashfs_offset(path, arch):
    header = elf_header(path, arch)
    if header[8:11] != b'AI\x02':
        raise ValueError(f'{path}: expected a type-2 AppImage')
    # Type-2 images append SquashFS after the ELF's last file-backed section.
    # Do not search for "hsqs": the runtime itself contains that byte sequence.
    if header[4] == 1:
        shoff = struct.unpack_from('<I', header, 32)[0]
        entsize, count = struct.unpack_from('<HH', header, 46)
        section_format = '<10I'
    else:
        shoff = struct.unpack_from('<Q', header, 40)[0]
        entsize, count = struct.unpack_from('<HH', header, 58)
        section_format = '<IIQQQQIIQQ'
    size = Path(path).stat().st_size
    if not count or entsize != struct.calcsize(section_format) or shoff + entsize * count > size:
        raise ValueError(f'{path}: invalid ELF section table')
    end = shoff + entsize * count
    with Path(path).open('rb') as stream:
        stream.seek(shoff)
        for _ in range(count):
            section = struct.unpack(section_format, stream.read(entsize))
            if section[1] != 8:  # SHT_NOBITS occupies memory, not file space.
                end = max(end, section[4] + section[5])
        stream.seek(end)
        if stream.read(4) != b'hsqs':
            raise ValueError(f'{path}: no SquashFS at ELF end {end}')
    return end


def extract(path, arch, destination):
    offset = squashfs_offset(path, arch)
    subprocess.run(['unsquashfs', '-no-progress', '-o', str(offset),
                    '-d', str(destination), str(path)], check=True)


if __name__ == '__main__':
    extract(Path(sys.argv[1]), sys.argv[2], Path(sys.argv[3]))
