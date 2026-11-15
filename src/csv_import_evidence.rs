//! Read-only interpretation of original CSV evidence. Current settings never
//! reinterpret a retained row. Missing audit/archive evidence is not a zero.
use crate::{
    csv_import::HoursMethod, import_service::ImportInterpretation, models::TimesheetEntry,
};
use rusqlite::{Connection, OptionalExtension};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SourceMetadata {
    pub method: Option<HoursMethod>,
    pub rate_supplied: Option<bool>,
    pub amount_supplied: Option<bool>,
}
impl SourceMetadata {
    fn legacy() -> Self {
        Self {
            method: Some(HoursMethod::WorkedHours),
            rate_supplied: Some(true),
            amount_supplied: Some(true),
        }
    }
    pub fn money(&self, raw: &TimesheetEntry) -> (String, String) {
        (
            money(raw.hourly_rate, self.rate_supplied),
            money(raw.amount, self.amount_supplied),
        )
    }
}
fn money(value: f64, supplied: Option<bool>) -> String {
    match supplied {
        Some(true) => format!("£{value:.2}"),
        Some(false) => "Missing".into(),
        None => "Unavailable".into(),
    }
}

pub(crate) fn source_metadata(
    db: &Connection,
    raw: &TimesheetEntry,
) -> Result<SourceMetadata, Box<dyn std::error::Error>> {
    // Use the earliest original import, not a later re-export with fewer fields.
    let audit: Option<(String, String, Option<String>)> = db
        .query_row(
            "SELECT c.sha256,a.archive_filename,e.after_evidence FROM csv_import_rows r
         JOIN csv_import_contents c ON c.sha256=r.sha256
         JOIN import_audit a ON a.id=c.import_audit_id
         LEFT JOIN shift_change_events e ON e.source='csv_import' AND e.source_id=a.id
         WHERE r.source_id=?1 ORDER BY a.id,e.id LIMIT 1",
            [raw.id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let Some((digest, archive, Some(text))) = audit else {
        return Ok(SourceMetadata::legacy());
    };
    let interpretation: ImportInterpretation = toml::from_str(&text)?;
    let method = if interpretation.legacy {
        HoursMethod::WorkedHours
    } else {
        interpretation.config.hours_method
    };
    let mut result = SourceMetadata {
        method: Some(method),
        rate_supplied: None,
        amount_supplied: None,
    };
    if let Some((_, fields)) = interpretation
        .source_fields
        .iter()
        .find(|(id, _)| *id == raw.id)
    {
        result.rate_supplied = Some(fields[5]);
        result.amount_supplied = Some(fields[6]);
        return Ok(result);
    }
    // Compatibility with the initial Stage 1 audit format: replay only verified
    // archived bytes with their retained configuration. Never use current config.
    let Ok(bytes) = std::fs::read(archive) else {
        return Ok(result);
    };
    if crate::shift_changes::digest(&bytes) != digest {
        return Ok(result);
    }
    let rows = if interpretation.legacy {
        crate::csv_import::import_csv_bytes(&bytes)
    } else {
        crate::csv_import::import_csv_with_config(&bytes, &interpretation.config)
    }?;
    let mut candidates = Vec::new();
    for mut row in rows {
        // Replay cannot assign another PA's otherwise identical row to this ID.
        if row.entry.pa_name != raw.pa_name {
            continue;
        }
        row.entry.personal_assistant_id = raw.personal_assistant_id;
        if crate::import_service::existing_row_is_materially_identical(raw, &row) {
            let fields = interpretation
                .supplied_fields
                .iter()
                .find(|(line, _)| *line == row.source_row)
                .map_or(row.supplied_fields, |(_, fields)| *fields);
            candidates.push(fields);
        }
    }
    // Ambiguous identical rows with differing presence must not fabricate zero.
    let agreed = |field: usize| {
        candidates.first().and_then(|first| {
            candidates
                .iter()
                .all(|c| c[field] == first[field])
                .then_some(first[field])
        })
    };
    result.rate_supplied = agreed(5);
    result.amount_supplied = agreed(6);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app::Application,
        csv_import::{CsvImportConfig, HoursMethod},
    };

    fn setup() -> (crate::private_storage::PrivateTempDir, Application) {
        let (dir, app) = crate::payroll_timesheet_screen::tests::test_application();
        app.repository.connection().execute_batch("INSERT INTO personal_assistants(id,first_name,surname,employment_status) VALUES(1,'Example','PA','Active')").unwrap();
        std::fs::create_dir_all(&app.context.config.folders.csv_import).unwrap();
        (dir, app)
    }
    fn import(app: &Application, name: &str, csv: &str) -> crate::models::TimesheetEntry {
        std::fs::write(app.context.config.folders.csv_import.join(name), csv).unwrap();
        let result = app.create_import_service().run().unwrap();
        assert_eq!(result.files_succeeded, 1, "{:?}", result.failure_messages);
        app.repository.get_all_raw().unwrap().pop().unwrap()
    }
    #[test]
    fn historical_methods_control_warnings_after_settings_change_and_corrections() {
        for method in HoursMethod::ALL {
            let (_dir, mut app) = setup();
            app.context.config.csv_import.hours_method = method;
            let csv = "period\nPA,Start,End,Break,Worked\nExample PA,2 April 2026 at 09:00,2 April 2026 at 09:34,0h 10m,0h 24m\n";
            let csv = if method == HoursMethod::WorkedHoursMinusBreak { csv.replace("0h 24m", "0h 34m") } else { csv.to_string() };
            let raw = import(&app, "original.csv", &csv);
            app.context.config.csv_import.hours_method = if method == HoursMethod::WorkedHours {
                HoursMethod::EndMinusStart
            } else {
                HoursMethod::WorkedHours
            };
            let before =
                crate::database_recovery::fingerprint(app.repository.connection()).unwrap();
            let mut snapshot =
                crate::imported_hours::snapshot(app.repository.connection(), raw.id).unwrap();
            assert_eq!(snapshot.source_metadata.method, Some(method));
            assert!(crate::imported_hours::consistency_warning(&snapshot).is_none());
            snapshot.entry.effective.worked_minutes = 23;
            let warning = crate::imported_hours::consistency_warning(&snapshot).unwrap();
            assert!(warning.contains(match method {
                HoursMethod::EndMinusStart => "without break deduction",
                HoursMethod::EndMinusStartMinusBreak => "End minus Start minus Break",
                _ => "elapsed minus breaks",
            }));
            assert_eq!(
                crate::database_recovery::fingerprint(app.repository.connection()).unwrap(),
                before
            );
        }
    }
    #[test]
    fn omitted_money_and_supplied_zero_remain_distinct_even_for_identical_rows() {
        let (_dir, app) = setup();
        let csv = "period\nPA,Start,End,Worked,Rate,Amount\nExample PA,2 April 2026 at 09:00,2 April 2026 at 09:34,0h 34m,,\nExample PA,2 April 2026 at 09:00,2 April 2026 at 09:34,0h 34m,0,0\nExample PA,2 April 2026 at 09:40,2 April 2026 at 09:50,0h 10m,0,\nExample PA,2 April 2026 at 10:00,2 April 2026 at 10:10,0h 10m,,0\n";
        import(&app, "mixed.csv", csv);
        let rows = app.repository.get_all_raw().unwrap();
        let mut ui = crate::imported_hours_ui::ImportedHoursUi::default();
        let values: Vec<_> = rows
            .iter()
            .map(|r| ui.source_money(app.repository.connection(), r.id))
            .collect();
        assert!(values.contains(&("Missing".into(), "Missing".into())));
        assert!(values.contains(&("£0.00".into(), "£0.00".into())));
        assert!(values.contains(&("£0.00".into(), "Missing".into())));
        assert!(values.contains(&("Missing".into(), "£0.00".into())));
        assert!(rows.iter().all(|r| r.hourly_rate == 0.0 && r.amount == 0.0));
        let evidence =
            crate::payroll_evidence::load_connection(app.repository.connection()).unwrap();
        assert_eq!(crate::payroll_evidence::groups(&evidence).unwrap().len(), 1);
        // Direct per-source metadata does not depend on archive availability.
        let archive: String = app
            .repository
            .connection()
            .query_row(
                "SELECT archive_filename FROM import_audit WHERE status='SUCCESS'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        std::fs::write(archive, b"synthetic damaged archive").unwrap();
        for r in &rows {
            let metadata = source_metadata(app.repository.connection(), r).unwrap();
            assert!(metadata.rate_supplied.is_some());
        }
    }
    fn old_audit(
        app: &Application,
        config: CsvImportConfig,
        csv: &str,
        metadata: bool,
    ) -> TimesheetEntry {
        let bytes = csv.as_bytes();
        let mut rows = if metadata {
            crate::csv_import::import_csv_with_config(bytes, &config).unwrap()
        } else {
            crate::csv_import::import_csv_bytes(bytes).unwrap()
        };
        let flags: Vec<_> = rows
            .iter()
            .map(|r| (r.source_row, r.supplied_fields))
            .collect();
        for row in &mut rows {
            row.entry.personal_assistant_id = Some(if row.entry.pa_name == "Other PA" {
                2
            } else {
                1
            });
        }
        let archive = app.context.environment.archive_dir.join("old-stage1.csv");
        std::fs::create_dir_all(archive.parent().unwrap()).unwrap();
        std::fs::write(&archive, bytes).unwrap();
        let entries: Vec<_> = rows.into_iter().map(|r| r.entry).collect();
        app.repository
            .import_file_atomically(
                &entries,
                "historical",
                "historical.csv",
                archive.to_str().unwrap(),
                entries.len() as i64,
                0,
            )
            .unwrap();
        let db = app.repository.connection();
        let audit: i64 = db
            .query_row("SELECT MAX(id) FROM import_audit", [], |r| r.get(0))
            .unwrap();
        let hash = crate::shift_changes::digest(bytes);
        db.execute(
            "INSERT INTO csv_import_contents VALUES(?1,?2,'historical.csv')",
            rusqlite::params![hash, audit],
        )
        .unwrap();
        let raw = app.repository.get_all_raw().unwrap().remove(0);
        db.execute(
            "INSERT INTO csv_import_rows VALUES(?1,?2)",
            rusqlite::params![hash, raw.id],
        )
        .unwrap();
        if metadata {
            // Deliberately serialize the original Stage 1 shape without source_fields.
            #[derive(serde::Serialize)]
            struct OldInterpretation {
                config: CsvImportConfig,
                supplied_fields: Vec<(u64, [bool; 8])>,
                legacy: bool,
            }
            let text = toml::to_string(&OldInterpretation {
                config,
                supplied_fields: flags,
                legacy: false,
            })
            .unwrap();
            let text = text.lines().filter(|l| !l.starts_with("enabled =")).collect::<Vec<_>>().join("\n");
            db.execute("INSERT INTO shift_change_events(source,source_id,before_evidence,after_evidence,reason,review_signature,recorded_at,actor) VALUES('csv_import',?1,'',?2,'original stage1 audit',?3,'old','synthetic')",rusqlite::params![audit,text,hash]).unwrap();
        }
        raw
    }
    #[test]
    fn original_stage1_audit_uses_verified_archive_and_retained_custom_mapping() {
        let (_dir, mut app) = setup();
        let mut config = CsvImportConfig {
            introductory_rows: 0,
            hours_method: HoursMethod::EndMinusStart,
            ..Default::default()
        };
        config.headings[0] = vec!["Worker".into()];
        let raw=old_audit(&app,config,"Worker,Start,End,Break\nExample PA,2 April 2026 at 09:00,2 April 2026 at 09:34,0h 10m\n",true);
        app.context.config.csv_import = CsvImportConfig::default();
        let snapshot =
            crate::imported_hours::snapshot(app.repository.connection(), raw.id).unwrap();
        assert!(crate::imported_hours::consistency_warning(&snapshot).is_none());
        assert_eq!(
            snapshot.source_metadata.money(&raw),
            ("Missing".into(), "Missing".into())
        );
        let before = crate::database_recovery::fingerprint(app.repository.connection()).unwrap();
        let archive = app.context.environment.archive_dir.join("old-stage1.csv");
        std::fs::write(archive, b"synthetic tampering").unwrap();
        let unavailable = source_metadata(app.repository.connection(), &raw).unwrap();
        assert_eq!(
            unavailable.money(&raw),
            ("Unavailable".into(), "Unavailable".into())
        );
        assert_eq!(unavailable.method, Some(HoursMethod::EndMinusStart));
        assert_eq!(
            crate::database_recovery::fingerprint(app.repository.connection()).unwrap(),
            before
        );
    }
    #[test]
    fn legacy_import_keeps_supplied_zero_and_worked_hours_comparison() {
        let (_dir, mut app) = setup();
        let raw=old_audit(&app,Default::default(),"period\nPA,Start,End,Break,Worked,Rate,Amount,Note\nExample PA,2 April 2026 at 09:00,2 April 2026 at 09:34,0h 10m,0h 24m,0,0,\n",false);
        app.context.config.csv_import.hours_method = HoursMethod::EndMinusStart;
        let snapshot =
            crate::imported_hours::snapshot(app.repository.connection(), raw.id).unwrap();
        assert!(crate::imported_hours::consistency_warning(&snapshot).is_none());
        assert_eq!(
            snapshot.source_metadata.money(&raw),
            ("£0.00".into(), "£0.00".into())
        );
    }
    #[test]
    fn original_stage1_ambiguous_identical_rows_never_infer_zero_or_missing() {
        let (_dir, app) = setup();
        let raw=old_audit(&app,Default::default(),"period\nPA,Start,End,Worked,Rate,Amount\nExample PA,2 April 2026 at 09:00,2 April 2026 at 09:34,0h 34m,,\nExample PA,2 April 2026 at 09:00,2 April 2026 at 09:34,0h 34m,0,0\n",true);
        let metadata = source_metadata(app.repository.connection(), &raw).unwrap();
        assert_eq!(
            metadata.money(&raw),
            ("Unavailable".into(), "Unavailable".into())
        );
        assert_eq!(metadata.method, Some(HoursMethod::WorkedHours));
    }
    #[test]
    fn original_stage1_worked_hours_and_cross_pa_presence_ignore_current_settings() {
        let (_dir, mut app) = setup();
        app.repository
            .connection()
            .execute_batch(
                "INSERT INTO personal_assistants(id,first_name,surname) VALUES(2,'Other','PA')",
            )
            .unwrap();
        let raw=old_audit(&app,Default::default(),"period\nPA,Start,End,Break,Worked,Rate,Amount\nExample PA,2 April 2026 at 09:00,2 April 2026 at 09:34,0h 10m,0h 24m,,\nOther PA,2 April 2026 at 09:00,2 April 2026 at 09:34,0h 10m,0h 24m,0,0\n",true);
        app.context.config.csv_import.hours_method = HoursMethod::EndMinusStart;
        let snapshot =
            crate::imported_hours::snapshot(app.repository.connection(), raw.id).unwrap();
        assert_eq!(
            snapshot.source_metadata.money(&raw),
            ("Missing".into(), "Missing".into())
        );
        assert!(crate::imported_hours::consistency_warning(&snapshot).is_none());
    }
}
