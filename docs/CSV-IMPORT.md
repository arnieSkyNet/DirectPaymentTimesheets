# Configurable hours CSV import (v1.0.7 Stage 1)

Payroll Settings → CSV Import controls **CSV rows to skip** (physical lines before
the heading row), heading aliases, enabled columns and the hours calculation method.
Defaults retain one introductory line, all columns enabled, the existing Hours
Keeper/synthetic aliases and CSV Worked Hours. Headings ignore case, surrounding
whitespace and a leading BOM. Column order is irrelevant; disabled and unknown
columns are ignored, including invalid values in those ignored columns. Use `|`
between aliases. Enabled aliases must be unique across enabled fields.

PA, Start and End are required and locked on for every method. Dates/times use the
existing supported formats and End must be later than Start. Worked Hours is
required and locked on for methods 1 and 2. Break is required and locked on for
methods 2 and 4. Selecting a method enables its required fields immediately; fields
that become optional remain enabled until explicitly unchecked. Disabled editors
retain their aliases for re-enabling. Saving validates required mappings before
publishing the draft. The compact horizontal spreadsheet preview shows enabled
fields and the first alias of each, updating immediately; it is an illustration,
**not a prescribed CSV column order**. Scroll it horizontally if needed.

1. **CSV Worked Hours** (unchanged default): use the supplied duration,
   independently of clocks and breaks.
2. **CSV Worked Hours minus Break**: subtract the explicit Break value from the
   supplied Worked Hours.
3. **End minus Start (no break deduction)**: use elapsed whole minutes.
4. **End minus Start minus Break**: subtract the explicit Break value from elapsed
   whole minutes.

Both break-deducting methods require an enabled, mapped Break column and a valid,
non-negative value on **every shift**, including `0h 00m` for no break. Missing,
blank, malformed, negative, overflowing and excessive breaks refuse the entire
file; a break cannot exceed elapsed shift duration or the selected base hours.
A zero calculated duration is allowed; negative calculated durations are rejected.
In methods 1 and 3, optional Break may be omitted/blank (zero) and is not deducted.
Worked Hours does not control duration in methods 3 and 4. Disabled Worked Hours
is ignored; an enabled but unused Worked Hours column is retained as source
presence without being parsed as the calculation input, as before.

Rate, Amount and Note are always optional. Enabled supplied money must be finite
and non-negative. Omitted/disabled numeric evidence normalizes to zero only in the
existing source-row schema; Rate and Amount display as **Missing**, distinguished
from a supplied zero. Note normalizes to none. Rate/Amount remain source evidence,
not the maintained payroll rate. Raw archived bytes, the immutable configuration
(including method and enabled-column flags) and per-source supplied-field flags
retain the original interpretation, including the pre-deduction Worked Hours.

All four methods leave durations **unrounded at import**. Existing payroll rounding
applies once to each calculated shift at preparation, before weekly summation.
PDF formatting remains unchanged. Discrepancy warnings use the original import's
retained method: the legacy/default Worked Hours comparison remains elapsed minus
break; Worked Hours minus Break and End minus Start minus Break compare the stored
net duration against elapsed minus break; End minus Start compares against elapsed.
Later settings changes do not reinterpret historical imports or corrections.

Existing raw rows, corrections, submissions and archived CSV bytes are not
rewritten. Recognized bytes retain their first interpretation, including when
copied to another pathname. Unchanged-file PA coverage is reconstructed from
retained raw membership, not today's settings. Legacy archive reconstruction
continues to use the original strict eight-field parser. Changing settings affects
new content; changed exports can create reviewable candidates rather than replace
retained rows. Existing exact-repeat/correction comparisons remain unchanged,
including Rate, Amount and Note comparisons. Consequently exporting the same work
with optional evidence removed can require duplicate review.

Import interpretation is recorded atomically with successful import in the
existing immutable `shift_change_events` audit (`source = csv_import`, source ID
is the import audit ID); no database schema migration is introduced. Clipboard
validation and folder imports share the saved settings, including all four methods
and column-enable flags. A clipboard session refuses to
continue if settings change before completion.

Rate and Amount display as **Missing** when omitted or blank, while supplied zero
continues to display as £0.00. The source-row numeric schema and exact-repeat
comparisons remain unchanged. New immutable interpretation audits map supplied
fields directly to source IDs, including otherwise identical duplicate rows.
Initial Stage 1 audits without those IDs use their retained configuration and
hash-verified archive to recover presence; ambiguous or unavailable evidence is
displayed as Unavailable rather than inferred as zero. Legacy strict eight-column
imports retain their required monetary values. No audit or archive is rewritten.
The persisted `[csv_import]` configuration key is unchanged by the settings move.

Older `[csv_import]` settings and interpretation audits without `enabled` load with
all eight fields enabled, preserving their prior interpretation. New audits carry
the complete enable flags and method through the existing immutable import event;
no schema migration or historical metadata rewrite is introduced.
