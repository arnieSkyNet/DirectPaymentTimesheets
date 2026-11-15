#!/usr/bin/env python3
"""Build a read-only, operator-reviewed schema-28 recovery inventory.

Requires Python's sqlite3 module. Never opens either database for writing.
Additional direct IDs require independently established pre-cutover provenance;
work dates and database row counts alone are NOT proof of evidence existence.
"""
import argparse
import datetime
import json
from pathlib import Path
import sqlite3


def clock(value):
    for fmt in ('%Y-%m-%dT%H:%M:%S', '%Y-%m-%dT%H:%M', '%d %B %Y at %H:%M:%S'):
        try:
            return datetime.datetime.strptime(value, fmt).isoformat(timespec='seconds')
        except ValueError:
            pass
    return value  # Retain date-only evidence; never invent an interval.


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('database', type=Path)
    p.add_argument('backup', type=Path)
    p.add_argument('output', type=Path)
    p.add_argument('--confirmed-direct-id', type=int, action='append', default=[])
    p.add_argument('--provenance', required=True)
    args = p.parse_args()
    live = sqlite3.connect(args.database.resolve().as_uri() + '?mode=ro', uri=True)
    backup = sqlite3.connect(args.backup.resolve().as_uri() + '?mode=ro', uri=True)
    if live.execute('SELECT version FROM schema_version').fetchone()[0] != 28:
        raise SystemExit('Recovery inventory requires a schema-28 database')
    if backup.execute('SELECT version FROM schema_version').fetchone()[0] >= 28:
        raise SystemExit('Backup must predate schema 28')
    # Compare the complete immutable imported data, not just counts or maximum IDs.
    originals = backup.execute('SELECT * FROM timesheets ORDER BY id').fetchall()
    if originals != live.execute('SELECT * FROM timesheets ORDER BY id').fetchall():
        raise SystemExit('Imported inventories differ; explicit evidence investigation required')
    ids = {r[0] for r in backup.execute('SELECT id FROM direct_shifts')}
    ids.update(args.confirmed_direct_id)
    existing = {r[0] for r in live.execute('SELECT id FROM direct_shifts')}
    if not ids <= existing:
        raise SystemExit('A confirmed direct source no longer exists')
    evidence = []
    query = '''SELECT t.id,t.personal_assistant_id,t.pa_name,
        COALESCE(c.after_start_time,t.start_time),COALESCE(c.after_end_time,t.end_time),
        COALESCE(c.after_break_minutes,t.break_minutes),COALESCE(c.after_worked_minutes,t.worked_minutes),
        CASE WHEN c.id IS NULL THEN t.notes ELSE c.after_notes END
        FROM timesheets t LEFT JOIN timesheet_correction_events c ON c.id=(SELECT MAX(id) FROM timesheet_correction_events WHERE timesheet_id=t.id)
        WHERE t.personal_assistant_id IS NOT NULL ORDER BY t.id'''
    for ident, pa, name, start, end, brk, minutes, notes in live.execute(query):
        evidence.append(dict(source='imported', id=ident, pa=pa, pa_name=name, start=clock(start), end=clock(end), break_minutes=brk, minutes=minutes, notes=notes or '', deleted=False))
    for ident, pa, start, end, brk, notes, deleted in live.execute('SELECT id,personal_assistant_id,start_time,end_time,break_minutes,notes,deleted_at FROM direct_shifts WHERE end_time IS NOT NULL ORDER BY id'):
        if ident not in ids:
            continue
        start, end = clock(start), clock(end)
        minutes = int((datetime.datetime.fromisoformat(end)-datetime.datetime.fromisoformat(start)).total_seconds()/60)-brk
        evidence.append(dict(source='direct', id=ident, pa=pa, pa_name='', start=start, end=end, break_minutes=brk, minutes=minutes, notes=notes or '', deleted=deleted is not None))
    settlements = live.execute("SELECT p.id,e.sent_at FROM payroll_timesheets p JOIN payroll_timesheet_email_status e ON e.personal_assistant_id=p.personal_assistant_id AND e.payroll_year=p.payroll_year AND e.cycle_number=p.cycle_number WHERE e.email_type='payslip' AND e.sent_at IS NOT NULL AND substr(e.sent_at,1,14)<>'indeterminate:' ORDER BY p.id").fetchall()
    provenance = f'{args.provenance}; backup: {args.backup}; complete imported rows compared: {len(originals)}; independently confirmed additional direct IDs: {args.confirmed_direct_id}. Settlement inventory reflects the operator-confirmed cutover statuses, not an inferred migration timestamp.'
    lines = ['provenance = ' + json.dumps(provenance)]
    for entry in evidence:
        lines.append('\n[[evidence]]')
        lines.extend(f'{k} = {json.dumps(v)}' for k, v in entry.items())
    if not evidence:
        raise SystemExit('No confirmed evidence')
    for ident, sent in settlements:
        lines.extend(['\n[[settled]]', f'record = {ident}', 'sent_at = ' + json.dumps(sent)])
    if not settlements:
        lines.insert(1, 'settled = []')
    with args.output.open('x') as output:
        output.write('\n'.join(lines)+'\n')
    print(f'Wrote {len(evidence)} evidence identities and {len(settlements)} definitive settlement records to {args.output}')


if __name__ == '__main__':
    main()
