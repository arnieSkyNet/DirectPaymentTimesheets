CREATE TABLE payroll_duplicate_decisions (
 id INTEGER PRIMARY KEY, group_fingerprint TEXT NOT NULL,
 winner_source TEXT NOT NULL, winner_id INTEGER NOT NULL,
 decided_at TEXT NOT NULL, actor TEXT NOT NULL,
 invalidated_at TEXT, invalidation_reason TEXT
);
CREATE UNIQUE INDEX payroll_duplicate_active ON payroll_duplicate_decisions(group_fingerprint) WHERE invalidated_at IS NULL;
CREATE TABLE payroll_duplicate_members (
 decision_id INTEGER NOT NULL, source TEXT NOT NULL, source_id INTEGER NOT NULL,
 fingerprint TEXT NOT NULL, evidence TEXT NOT NULL,
 PRIMARY KEY(decision_id, source, source_id)
);
CREATE TABLE payroll_submissions (
 id INTEGER PRIMARY KEY, payroll_timesheet_id INTEGER NOT NULL,
 submitted_at TEXT, supersedes_id INTEGER,
 pdf_path TEXT NOT NULL, pdf_sha256 TEXT NOT NULL, pdf_bytes BLOB
);
CREATE TABLE payroll_submission_items AS SELECT 0 AS submission_id, * FROM payroll_timesheet_worked_item_snapshots WHERE 0;
CREATE TABLE payroll_submission_weeks AS SELECT 0 AS submission_id, * FROM payroll_timesheet_weeks WHERE 0;
CREATE TABLE payroll_submission_leave AS SELECT 0 AS submission_id, * FROM payroll_timesheet_annual_leave WHERE 0;
CREATE TABLE payroll_submission_holidays AS SELECT 0 AS submission_id, * FROM payroll_timesheet_public_holidays WHERE 0;
CREATE TABLE payroll_reconciliation_decisions (
 id INTEGER PRIMARY KEY, payroll_timesheet_id INTEGER NOT NULL, submission_id INTEGER,
 kind TEXT NOT NULL CHECK(kind IN ('resubmit','carry','complete_actuals','already_paid','unpaid')),
 evidence_key TEXT NOT NULL, evidence TEXT NOT NULL, decided_at TEXT NOT NULL, actor TEXT NOT NULL,
 UNIQUE(payroll_timesheet_id,kind,evidence_key)
);
CREATE TABLE payroll_corrections (
 id INTEGER PRIMARY KEY, personal_assistant_id INTEGER NOT NULL,
 origin_payroll_timesheet_id INTEGER NOT NULL, submission_id INTEGER,
 decision_id INTEGER, evidence_key TEXT NOT NULL UNIQUE,
 source TEXT, source_id INTEGER, work_date TEXT,
 minutes INTEGER NOT NULL CHECK(minutes <> 0),
 pay_rate_id INTEGER, pay_rate_effective_date TEXT, total_hourly_rate REAL,
 reason TEXT NOT NULL, created_at TEXT NOT NULL
);
CREATE TABLE payroll_correction_applications (
 payroll_timesheet_id INTEGER NOT NULL, correction_id INTEGER NOT NULL,
 minutes INTEGER NOT NULL CHECK(minutes <> 0),
 PRIMARY KEY(payroll_timesheet_id,correction_id)
);
CREATE TABLE payroll_submission_corrections (
 submission_id INTEGER NOT NULL, correction_id INTEGER NOT NULL, minutes INTEGER NOT NULL,
 PRIMARY KEY(submission_id,correction_id)
);
CREATE TABLE payroll_candidate_checks (
 payroll_timesheet_id INTEGER PRIMARY KEY, evidence_signature TEXT NOT NULL
);
CREATE TABLE payroll_correction_evidence (
 correction_id INTEGER NOT NULL, source TEXT NOT NULL, source_id INTEGER NOT NULL,
 fingerprint TEXT NOT NULL, evidence TEXT NOT NULL,
 PRIMARY KEY(correction_id,source,source_id)
);
