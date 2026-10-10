//! Review UI leaves the existing raw import filtering/sorting/scrolling intact.
use crate::{
    app::Application,
    imported_hours::{self, Change, Snapshot},
    repository::TimesheetCorrectionProposal,
};
#[derive(Default)]
pub struct ImportedHoursUi {
    editor: Option<Editor>,
    pub message: String,
    rows: std::collections::BTreeMap<i64, std::result::Result<(Snapshot, String), String>>,
    database_version: Option<(i64, i64)>,
}
struct Editor {
    expected: Snapshot,
    change: Change,
    reason: String,
    acknowledged: bool,
    token: Option<String>,
}
impl ImportedHoursUi {
    pub fn refresh(&mut self) {
        self.rows.clear();
        self.database_version = None;
    }
    pub fn begin_frame(&mut self, app: &Application) {
        let db = app.repository.connection();
        let version = (|| -> rusqlite::Result<(i64, i64)> {
            Ok((
                db.query_row("PRAGMA data_version", [], |r| r.get(0))?,
                db.query_row("SELECT total_changes()", [], |r| r.get(0))?,
            ))
        })();
        match version {
            Ok(v) => {
                if self.database_version != Some(v) {
                    self.rows.clear();
                    self.database_version = Some(v);
                }
            }
            Err(_) => {
                self.refresh();
            }
        }
    }
    pub fn open(&mut self, app: &Application, id: i64) {
        match imported_hours::snapshot(app.repository.connection(), id) {
            Ok(s) => {
                self.message.clear();
                self.editor = Some(Editor {
                    change: edit_change(&s),
                    expected: s,
                    reason: String::new(),
                    acknowledged: false,
                    token: None,
                });
            }
            Err(e) => {
                self.message = format!("Evidence review unavailable: {e}");
            }
        }
    }
    pub fn row(&mut self, ui: &mut egui::Ui, app: &Application, id: i64) {
        let db = app.repository.connection();
        if !self.rows.contains_key(&id) {
            let row = imported_hours::snapshot(db, id)
                .map(|snapshot| {
                    let status = imported_hours::payable_status(db, &snapshot)
                        .unwrap_or_else(|e| format!("Status unavailable: {e}"));
                    (snapshot, status)
                })
                .map_err(|e| e.to_string());
            self.rows.insert(id, row);
        }
        match self.rows.get(&id).cloned().expect("row inserted above") {
            Err(e) => {
                ui.label(format!("Evidence review unavailable: {e}"));
            }
            Ok((s, status)) => {
                ui.label(format!(
                    "Effective: {} – {}; break {}m; worked {}m",
                    s.entry.effective.start_time,
                    s.entry.effective.end_time,
                    s.entry.effective.break_minutes,
                    s.entry.effective.worked_minutes
                ));
                ui.label(status);
                if let Some(w) = imported_hours::consistency_warning(&s) {
                    ui.label(w);
                }
                ui.horizontal_wrapped(|ui| {
                    for (label, change) in [
                        (
                            "Edit",
                            Change::Edit(TimesheetCorrectionProposal {
                                start_time: s.entry.effective.start_time.clone(),
                                end_time: s.entry.effective.end_time.clone(),
                                break_minutes: s.entry.effective.break_minutes,
                                worked_minutes: s.entry.effective.worked_minutes,
                                notes: s.entry.effective.notes.clone(),
                            }),
                        ),
                        ("Revert to original", Change::Revert),
                        (
                            if s.excluded { "Restore" } else { "Exclude" },
                            if s.excluded {
                                Change::Restore
                            } else {
                                Change::Exclude
                            },
                        ),
                    ] {
                        if ui.button(label).clicked() {
                            self.message.clear();
                            self.editor = Some(Editor {
                                expected: s.clone(),
                                change,
                                reason: String::new(),
                                acknowledged: false,
                                token: None,
                            });
                        }
                    }
                });
                ui.collapsing(format!("Source, notes and history — imported:{id}"),|ui| {
                    ui.label(format!("Original Hours Keeper evidence: {}; PA ID {:?}; {} – {}; break {}m; worked {}m; rate £{:.2}; amount £{:.2}; notes: {}",id,s.entry.raw.personal_assistant_id,s.entry.raw.start_time,s.entry.raw.end_time,s.entry.raw.break_minutes,s.entry.raw.worked_minutes,s.entry.raw.hourly_rate,s.entry.raw.amount,s.entry.raw.notes.as_deref().unwrap_or("(none)")));
                    ui.label(format!("Effective notes: {}",s.entry.effective.notes.as_deref().unwrap_or("(none)")));
                    match imported_hours::review(db,&s,&Change::Edit(TimesheetCorrectionProposal{start_time:s.entry.effective.start_time.clone(),end_time:s.entry.effective.end_time.clone(),break_minutes:s.entry.effective.break_minutes,worked_minutes:s.entry.effective.worked_minutes,notes:s.entry.effective.notes.clone()})) {
                        Ok(r)=>{ui.label(format!("Affected prepared/retained payroll cycles: {}",if r.description.is_empty(){"None prepared yet — allocated by shift start date"}else{&r.description}));}
                        Err(e)=>{ui.label(format!("Payroll review: {e}"));}
                    }
                    if let Ok(history)=app.repository.correction_history(id){for e in history{ui.label(format!("Correction #{} — {} — {} — {}: {:?} → {:?}; reason: {}",e.id,e.action_at,e.actor_id,e.action_type,e.before,e.after,e.reason.as_deref().unwrap_or("(legacy reason not recorded)")));}}
                    let audit=(|| -> rusqlite::Result<Vec<String>> {
                        db.prepare("SELECT recorded_at||' — '||actor||' — '||CASE after_excluded WHEN 1 THEN 'Exclude' ELSE 'Restore' END||' — '||reason||' — authorised: '||authorised||char(10)||before_evidence||char(10)||after_evidence FROM imported_hours_inclusion_events WHERE timesheet_id=?1 ORDER BY id")?.query_map([id],|r|r.get(0))?.collect()
                    })();
                    match audit{Ok(lines)=>{for line in lines{ui.label(line);}},Err(e)=>{ui.label(format!("Inclusion history unavailable: {e}"));}}
                    let submissions=(|| -> rusqlite::Result<Vec<String>> {
                        db.prepare("SELECT DISTINCT 'Submission #'||s.id||' — '||p.payroll_year||' cycle '||p.cycle_number||' — '||COALESCE(s.submitted_at,'legacy timestamp unavailable')||' — PDF SHA256 '||s.pdf_sha256 FROM payroll_submission_items i JOIN payroll_submissions s ON s.id=i.submission_id JOIN payroll_timesheets p ON p.id=s.payroll_timesheet_id WHERE i.timesheet_id=?1 ORDER BY s.id")?.query_map([id],|r|r.get(0))?.collect()
                    })();
                    match submissions{Ok(lines)=>{for line in lines{ui.label(line);}},Err(e)=>{ui.label(format!("Submission history unavailable: {e}"));}}
                    let archives=(|| -> rusqlite::Result<Vec<String>> {
                        db.prepare("SELECT DISTINCT a.id||' — SHA256 '||c.sha256||' — '||a.archive_filename FROM csv_import_rows r JOIN csv_import_contents c ON c.sha256=r.sha256 JOIN import_audit a ON a.id=c.import_audit_id WHERE r.source_id=?1")?.query_map([id],|r|r.get(0))?.collect()
                    })();
                    if let Ok(lines)=archives{for line in lines{ui.label(format!("Source archive: {line}"));}}
                });
            }
        }
    }
    /// Returns true on successful mutation so dashboard preparation caches refresh.
    pub fn show(&mut self, ctx: &egui::Context, app: &Application) -> bool {
        let Some(editor) = self.editor.as_mut() else {
            return false;
        };
        let mut cancel = false;
        let mut save = false;
        egui::Window::new("Review imported hours correction").id(egui::Id::new("imported_hours_editor")).resizable(true).vscroll(true).show(ctx,|ui| {
            ui.label(format!("Hours Keeper row {}; {} (PA ID {:?})",editor.expected.entry.raw.id,editor.expected.entry.raw.pa_name,editor.expected.entry.raw.personal_assistant_id));
            ui.label(format!("Original source: {} – {}; break {}m; worked {}m; notes {}",editor.expected.entry.raw.start_time,editor.expected.entry.raw.end_time,editor.expected.entry.raw.break_minutes,editor.expected.entry.raw.worked_minutes,editor.expected.entry.raw.notes.as_deref().unwrap_or("(none)")));
            ui.label(format!("Currently effective: {:?}; excluded: {}",editor.expected.entry.effective,editor.expected.excluded));
            ui.horizontal_wrapped(|ui| {
                let options=[("Edit",edit_change(&editor.expected)),("Revert to original",Change::Revert),(if editor.expected.excluded{"Restore"}else{"Exclude"},if editor.expected.excluded{Change::Restore}else{Change::Exclude})];
                for (label,change) in options {
                    let selected=std::mem::discriminant(&editor.change)==std::mem::discriminant(&change);
                    if ui.selectable_label(selected,label).clicked() && !selected {editor.change=change;editor.acknowledged=false;editor.token=None;}
                }
            });

            let mut changed=false;
            match &mut editor.change {
                Change::Edit(p)=>{
                    ui.label("Start date/time (YYYY-MM-DDTHH:MM):");changed|=ui.text_edit_singleline(&mut p.start_time).changed();
                    ui.label("End date/time (YYYY-MM-DDTHH:MM):");changed|=ui.text_edit_singleline(&mut p.end_time).changed();
                    ui.horizontal(|ui|{ui.label("Break minutes:");changed|=ui.add(egui::DragValue::new(&mut p.break_minutes).range(0..=i64::MAX)).changed();});
                    ui.horizontal(|ui|{ui.label("Authoritative worked minutes:");changed|=ui.add(egui::DragValue::new(&mut p.worked_minutes).range(0..=i64::MAX)).changed();});
                    ui.label("Notes:");changed|=ui.text_edit_multiline(p.notes.get_or_insert_with(String::new)).changed();
                    if let Ok(v)=crate::repository::validate_and_normalize_proposal(p){let mut proposed=editor.expected.clone();proposed.entry.effective=v;if let Some(w)=imported_hours::consistency_warning(&proposed){ui.label(w);}}
                    ui.label(format!("Proposed: {p:?}"));
                }
                Change::Revert=>{ui.label("Append an audited reversion to the original dates, duration, breaks and notes. Explicit exclusion status remains unchanged.");}
                Change::Exclude=>{ui.label("Exclude this source from payable work. Original evidence and submissions remain retained; historical discrepancies require payroll reconciliation.");}
                Change::Restore=>{ui.label("Restore this source to duplicate and payroll review. Restoration does not bypass duplicate selection or make previously paid work payable again.");}
            }
            ui.label("Reason / explanation:");changed|=ui.text_edit_multiline(&mut editor.reason).changed();
            if changed{editor.acknowledged=false;}
            let mut allowed=false;
            match imported_hours::review(app.repository.connection(),&editor.expected,&editor.change){
                Err(e)=>{ui.colored_label(ui.visuals().error_fg_color,e.to_string());editor.acknowledged=false;editor.token=None;}
                Ok(review)=>{
                    if editor.token.as_ref()!=Some(&review.signature){editor.acknowledged=false;editor.token=Some(review.signature);}
                    ui.label(format!("Affected payroll: {}",if review.description.is_empty(){"No prepared cycles yet"}else{&review.description}));
                    allowed=!editor.reason.trim().is_empty();
                    if review.protected{
                        ui.label("Original submissions, PDFs and settled totals will remain unchanged. Review financial implications in Payroll Timesheet Preparation after saving.");
                        ui.checkbox(&mut editor.acknowledged,"I explicitly authorise the displayed protected-cycle source correction");
                        allowed &= editor.acknowledged;
                    }
                }
            }
            if !self.message.is_empty(){ui.label(&self.message);}
            ui.horizontal(|ui|{save=ui.add_enabled(allowed,egui::Button::new("Approve and save reviewed change")).clicked();cancel=ui.button("Cancel").clicked();});
        });
        if save {
            match imported_hours::apply(
                app.repository.connection(),
                &editor.expected,
                &editor.change,
                &editor.reason,
                editor.token.as_deref().unwrap_or(""),
                editor.acknowledged,
            ) {
                Ok(_) => {
                    self.message="Saved. Original evidence retained. Reopen preparation to review any discrepancy and regenerate affected unsent PDFs.".into();
                    self.editor = None;
                    self.rows.clear();
                    return true;
                }
                Err(e) => {
                    self.message = format!("Change refused: {e}");
                    editor.acknowledged = false;
                }
            }
        }
        if cancel {
            self.editor = None;
        }
        false
    }
}

fn edit_change(s: &Snapshot) -> Change {
    Change::Edit(TimesheetCorrectionProposal {
        start_time: s.entry.effective.start_time.clone(),
        end_time: s.entry.effective.end_time.clone(),
        break_minutes: s.entry.effective.break_minutes,
        worked_minutes: s.entry.effective.worked_minutes,
        notes: s.entry.effective.notes.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stage7_view_cache_observes_other_instance_and_local_corrections() {
        let (_dir, app) = crate::payroll_timesheet_screen::tests::test_application();
        let db = app.repository.connection();
        db.execute(
            "INSERT INTO personal_assistants(id,first_name,surname) VALUES(1,'Example','PA')",
            [],
        )
        .unwrap();
        app.repository
            .insert(&crate::models::TimesheetEntry {
                id: 0,
                pa_name: "Example PA".into(),
                personal_assistant_id: Some(1),
                start_time: "2 April 2026 at 09:00".into(),
                end_time: "2 April 2026 at 10:00".into(),
                break_minutes: 0,
                worked_minutes: 60,
                hourly_rate: 12.0,
                amount: 12.0,
                notes: None,
            })
            .unwrap();
        let context = egui::Context::default();
        let mut screen = ImportedHoursUi::default();
        let draw = |screen: &mut ImportedHoursUi| {
            let _ = context.run(Default::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    screen.begin_frame(&app);
                    screen.row(ui, &app, 1);
                });
            });
        };
        draw(&mut screen);
        assert_eq!(
            screen.rows[&1]
                .as_ref()
                .unwrap()
                .0
                .entry
                .effective
                .worked_minutes,
            60
        );
        let other = crate::payroll_evidence::open(&app).unwrap();
        let s = imported_hours::snapshot(&other, 1).unwrap();
        let c = Change::Edit(TimesheetCorrectionProposal {
            start_time: s.entry.effective.start_time.clone(),
            end_time: s.entry.effective.end_time.clone(),
            break_minutes: 0,
            worked_minutes: 53,
            notes: None,
        });
        let r = imported_hours::review(&other, &s, &c).unwrap();
        imported_hours::apply(&other, &s, &c, "external instance", &r.signature, false).unwrap();
        draw(&mut screen);
        assert_eq!(
            screen.rows[&1]
                .as_ref()
                .unwrap()
                .0
                .entry
                .effective
                .worked_minutes,
            53
        );
        let s = imported_hours::snapshot(db, 1).unwrap();
        let r = imported_hours::review(db, &s, &Change::Exclude).unwrap();
        imported_hours::apply(
            db,
            &s,
            &Change::Exclude,
            "local instance",
            &r.signature,
            false,
        )
        .unwrap();
        draw(&mut screen);
        assert!(screen.rows[&1].as_ref().unwrap().0.excluded);
    }
}
