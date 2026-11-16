#!/usr/bin/env python3
"""Check the entire lockfile against a supplied RustSec advisory-db tar.gz.

No code from the database is executed or extracted. Unknown range syntax fails
closed. Exceptions are exact package/version/advisory triples, with expiry and
review rationale. Download the current database separately from RustSec's GitHub.
"""
import argparse
import datetime
import hashlib
import json
from pathlib import Path
import re
import tarfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]


def version(value):
    match = re.fullmatch(r'(\d+)\.(\d+)\.(\d+)(?:-([\w.-]+))?(?:\+[\w.-]+)?', value)
    if not match:
        raise ValueError('Unsupported version: ' + value)
    pre = tuple((0, int(s)) if s.isdigit() else (1, s) for s in (match[4] or '').split('.'))
    return tuple(int(match[i]) for i in (1, 2, 3)) + (int(match[4] is None), pre)


def matches(value, requirement):
    current = version(value)
    prerelease_bounds = []
    for comparator in requirement.split(','):
        match = re.fullmatch(r'\s*(>=|<=|>|<|=|\^|~)?\s*(\d+(?:\.\d+){0,2}(?:-[\w.-]+)?)\s*', comparator)
        if not match:
            raise ValueError('Unsupported RustSec range: ' + requirement)
        raw = match[2]
        parts = raw.split('-', 1)[0].split('.')
        count = len(parts)
        if count < 3:
            if '-' in raw:
                raise ValueError('Partial prerelease comparator')
            raw += '.0' * (3 - count)
        op, bound = match[1] or '^', version(raw)
        if not bound[3]:
            prerelease_bounds.append(bound[:3])
        major, minor, patch = bound[:3]
        partial_upper = version(f'{major + 1}.0.0' if count == 1 else f'{major}.{minor + 1}.0')
        upper = version(f'{major + 1}.0.0' if major else f'0.{minor + 1}.0' if minor else f'0.0.{patch + 1}')
        if count < 3 and not any(bound[:count]):
            upper = partial_upper
        admitted = {'>=': current >= bound, '<=': current <= bound,
                    '>': current > bound, '<': current < bound, '=': current == bound,
                    '^': bound <= current < upper,
                    '~': bound <= current < version(f'{major}.{minor + 1}.0')}[op]
        if count < 3 and op in ('=', '<=', '>'):
            admitted = {'=': bound <= current < partial_upper,
                        '<=': current < partial_upper, '>': current >= partial_upper}[op]
        if count == 1 and op == '~':
            admitted = bound <= current < partial_upper
        if not admitted:
            return False
    return bool(current[3] or current[:3] in prerelease_bounds)


def findings(lock, archive):
    packages = {}
    for p in lock['package']:
        if p.get('source', '').startswith('registry+'):
            packages.setdefault(p['name'], []).append(p['version'])
    result = set()
    with tarfile.open(archive, 'r:gz') as tar:
        for member in tar:
            if '/crates/' not in member.name or not member.name.endswith('.md'):
                continue
            text = tar.extractfile(member).read().decode()
            if '```toml' not in text:
                raise ValueError('Malformed advisory metadata')
            data = tomllib.loads(text.split('```toml', 1)[1].split('```', 1)[0])
            advisory = data['advisory']
            if advisory.get('withdrawn'):
                continue
            ranges = data.get('versions', {})
            safe = ranges.get('patched', []) + ranges.get('unaffected', [])
            for v in packages.get(advisory['package'], []):
                if not any(matches(v, r) for r in safe):
                    result.add((advisory['id'], advisory['package'], v))
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('database', type=Path)
    args = parser.parse_args()
    today = datetime.date.today()
    policy = json.loads((ROOT / 'docs/dependency-advisory-exceptions.json').read_text())
    allowed = set()
    for entry in policy['exceptions']:
        if datetime.date.fromisoformat(entry['review_by']) < today or not entry['rationale'].strip():
            raise SystemExit('Expired/incomplete advisory exception: ' + entry['id'])
        for v in entry['versions']:
            triple = (entry['id'], entry['package'], v)
            if triple in allowed:
                raise SystemExit('Duplicate advisory exception')
            allowed.add(triple)
    actual = findings(tomllib.loads((ROOT / 'Cargo.lock').read_text()), args.database)
    for triple in sorted(actual):
        print(('REVIEWED' if triple in allowed else 'BLOCKED'), *triple)
    # A change in dependency resolution requires refreshing the exact policy,
    # rather than keeping broad ignores which can conceal future regressions.
    stale = allowed - actual
    unexpected = actual - allowed
    print('RustSec database SHA256:', hashlib.sha256(args.database.read_bytes()).hexdigest())
    print(f'{len(actual)} reviewed/matched triples; {len(unexpected)} unreviewed; {len(stale)} stale exceptions')
    if unexpected or stale:
        raise SystemExit(1)


if __name__ == '__main__':
    main()
