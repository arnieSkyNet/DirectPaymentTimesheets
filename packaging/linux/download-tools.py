#!/usr/bin/env python3
"""Download only checked-in, checksum-pinned tool assets; fail closed on changes."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys

from appimage import elf_header

arch, output = sys.argv[1:]
output = Path(output)
output.mkdir(parents=True, exist_ok=True)
for name, (url, expected) in json.loads(Path(__file__).with_name('tools.json').read_text())[arch].items():
    path = output / name
    subprocess.run(['curl', '--fail', '--location', '--retry', '3', '--output', str(path), url], check=True)
    with path.open('rb') as f:
        actual = hashlib.file_digest(f, 'sha256').hexdigest()
    if actual != expected:
        path.unlink()
        raise SystemExit(f'Checksum mismatch for {name}; refusing to execute it')
    if name != 'runtime-license':
        elf_header(path, arch)
    path.chmod(0o644 if name == 'runtime-license' else 0o755)
