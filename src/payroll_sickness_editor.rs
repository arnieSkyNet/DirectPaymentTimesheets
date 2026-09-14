use crate::app::Application;
use crate::payroll_timesheet_repository::{PayrollTimesheet, PayrollTimesheetWeek};
use crate::sickness_period_repository::{SicknessPeriod, SicknessPeriodRepository};
use chrono::{Datelike, NaiveDate};
use eframe::egui;
use std::collections::HashSet;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Default)]
pub(super) struct SicknessUi {
    populated_weeks: HashSet<i64>,
    editor: Option<Editor>,
}

struct Editor {
    record: PayrollTimesheet,
    week: String,
    name: String,
    rows: Vec<SicknessPeriod>,
    error: String,
    saved: Vec<SicknessPeriod>,
    confirm_close: bool,
    focus_first: bool,
}

impl Editor {
    fn new(
        record: PayrollTimesheet,
        week: String,
        name: String,
        rows: Vec<SicknessPeriod>,
        error: String,
    ) -> Self {
        let mut editor = Self {
            record,
            week,
            name,
            saved: rows.clone(),
            rows,
            error,
            confirm_close: false,
            focus_first: false,
        };
        if editor.rows.is_empty() && editor.error.is_empty() {
            editor.add_blank();
            editor.focus_first = true;
        }
        editor
    }

    fn add_blank(&mut self) {
        self.rows.push(SicknessPeriod {
            id: 0,
            personal_assistant_id: self.record.personal_assistant_id,
            start_date: String::new(),
            end_date: String::new(),
        });
    }

    fn has_unsaved(&self) -> bool {
        self.rows.iter().any(|row| {
            if row.id == 0 {
                return !row.start_date.trim().is_empty() || !row.end_date.trim().is_empty();
            }
            let Some(saved) = self.saved.iter().find(|saved| saved.id == row.id) else {
                return true;
            };
            match resolved_row(row, &self.week) {
                Ok(row) => row.start_date != saved.start_date || row.end_date != saved.end_date,
                Err(_) => true,
            }
        })
    }

    fn request_close(&mut self) -> bool {
        self.confirm_close = self.has_unsaved();
        !self.confirm_close
    }

    fn accept_saved(&mut self, index: usize, saved: Option<SicknessPeriod>) {
        let id = self.rows[index].id;
        self.saved.retain(|row| row.id != id);
        if let Some(saved) = saved {
            self.saved.push(saved.clone());
            self.rows[index] = saved;
        } else {
            self.rows.remove(index);
        }
    }
}

fn contextual_date(value: &str, week: &str) -> std::result::Result<NaiveDate, String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 2 || !value.bytes().all(|c| c.is_ascii_digit()) {
        return crate::date_utils::parse_input(value);
    }
    let day: u32 = value.parse().map_err(|_| "Invalid day".to_string())?;
    let start = crate::date_utils::parse_legacy(week)?;
    let end = start + chrono::Duration::days(6);
    let first = NaiveDate::from_ymd_opt(start.year(), start.month(), day);
    if (start.year(), start.month()) == (end.year(), end.month()) {
        return first.ok_or_else(|| {
            "That day does not exist in this week's month. Enter a full date.".into()
        });
    }
    let candidates: Vec<_> = [first, NaiveDate::from_ymd_opt(end.year(), end.month(), day)]
        .into_iter()
        .flatten()
        .collect();
    let within: Vec<_> = candidates
        .iter()
        .copied()
        .filter(|date| *date >= start && *date <= end)
        .collect();
    if within.len() == 1 {
        return Ok(within[0]);
    }
    let nearby: Vec<_> = candidates
        .into_iter()
        .filter(|date| {
            *date >= start - chrono::Duration::days(6) && *date <= end + chrono::Duration::days(6)
        })
        .collect();
    if nearby.len() == 1 {
        return Ok(nearby[0]);
    }
    Err("Cannot resolve that day unambiguously near this payroll week. Enter a full date.".into())
}

fn resolved_row(row: &SicknessPeriod, week: &str) -> Result<SicknessPeriod> {
    let start = contextual_date(&row.start_date, week)?;
    let end = contextual_date(&row.end_date, week)?;
    if end < start {
        return Err("To date cannot precede From date. Enter full dates if the period extends into another month.".into());
    }
    Ok(SicknessPeriod {
        start_date: crate::date_utils::iso(start),
        end_date: crate::date_utils::iso(end),
        ..row.clone()
    })
}

fn edit_date(ui: &mut egui::Ui, value: &mut String, focus: bool) {
    // Editor-local buffer: day-only drafts must not be flagged by the global full-date widget.
    let response = ui.add(
        egui::TextEdit::singleline(value)
            .desired_width(155.0)
            .hint_text("Day or full date"),
    );
    if focus {
        response.request_focus();
    }
}

fn repository(app: &Application) -> Result<SicknessPeriodRepository> {
    Ok(SicknessPeriodRepository::new(
        crate::payroll_evidence::open(app)?,
    ))
}

fn week_periods(app: &Application, pa: i64, week: &str) -> Result<Vec<SicknessPeriod>> {
    let start = crate::date_utils::parse_legacy(week)?;
    let end = start + chrono::Duration::days(6);
    Ok(repository(app)?.get_overlapping_for_pa(
        pa,
        &crate::date_utils::iso(start),
        &crate::date_utils::iso(end),
    )?)
}

impl SicknessUi {
    pub(super) fn refresh(
        &mut self,
        app: &Application,
        records: &[(PayrollTimesheet, Vec<PayrollTimesheetWeek>, String)],
    ) -> Result<()> {
        let mut populated = HashSet::new();
        for (record, weeks, _) in records {
            for week in weeks {
                if !week_periods(app, record.personal_assistant_id, &week.week_commencing)?
                    .is_empty()
                {
                    populated.insert(week.id);
                }
            }
        }
        self.populated_weeks = populated;
        Ok(())
    }

    pub(super) fn has_data(&self, week: i64) -> bool {
        self.populated_weeks.contains(&week)
    }

    pub(super) fn cell(
        &mut self,
        ui: &mut egui::Ui,
        app: &Application,
        record: &PayrollTimesheet,
        week: &PayrollTimesheetWeek,
        name: &str,
    ) {
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(65.0, ui.spacing().interact_size.y),
            egui::Sense::click(),
        );
        let populated = self.has_data(week.id);
        if response.hovered() && ui.is_enabled() {
            ui.painter()
                .rect_filled(rect, 3.0, ui.visuals().widgets.hovered.bg_fill);
        }
        let label = if populated {
            "•"
        } else if response.hovered() && ui.is_enabled() {
            "+"
        } else {
            ""
        };
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(14.0),
            ui.visuals().weak_text_color(),
        );
        let response = response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(if populated {
                "View or edit sickness dates"
            } else {
                "Add sickness dates"
            });
        if response.clicked() {
            let loaded = week_periods(app, record.personal_assistant_id, &week.week_commencing);
            let (rows, error) = match loaded {
                Ok(rows) => (rows, String::new()),
                Err(error) => (Vec::new(), error.to_string()),
            };
            self.editor = Some(Editor::new(
                record.clone(),
                week.week_commencing.clone(),
                name.into(),
                rows,
                error,
            ));
            if let Some(editor) = &mut self.editor {
                for row in &mut editor.rows {
                    row.start_date = crate::date_utils::preference(ui).display(&row.start_date);
                    row.end_date = crate::date_utils::preference(ui).display(&row.end_date);
                }
            }
        }
    }

    pub(super) fn show(
        &mut self,
        ui: &mut egui::Ui,
        app: &Application,
        records: &[(PayrollTimesheet, Vec<PayrollTimesheetWeek>, String)],
    ) {
        let Some(editor) = &mut self.editor else {
            return;
        };
        let mut close = false;
        let mut changed = false;
        egui::Modal::new(egui::Id::new("payroll_sickness_editor")).show(ui.ctx(), |ui| {
            ui.set_max_width(520.0);
            ui.heading(format!("Sickness — {}", editor.name));
            ui.label(format!(
                "Week commencing {}",
                crate::date_utils::screen(ui, &editor.week)
            ));
            ui.label("Enter a day number or full date. Save each period separately.");
            if editor.confirm_close {
                ui.label("There are unsaved sickness changes. Discard them?");
                ui.horizontal(|ui| {
                    if ui.button("Discard changes").clicked() {
                        close = true;
                    }
                    if ui.button("Cancel").clicked() {
                        editor.confirm_close = false;
                    }
                });
                return;
            }
            let mut action = None;
            egui::ScrollArea::vertical()
                .max_height(300.0)
                .show(ui, |ui| {
                    egui::Grid::new("sickness_dates").show(ui, |ui| {
                        ui.label("From");
                        ui.label("To");
                        ui.label("");
                        ui.end_row();
                        for (index, row) in editor.rows.iter_mut().enumerate() {
                            ui.push_id(index, |ui| {
                                edit_date(
                                    ui,
                                    &mut row.start_date,
                                    editor.focus_first && index == 0,
                                );
                            });
                            ui.push_id((index, "to"), |ui| {
                                edit_date(ui, &mut row.end_date, false);
                            });
                            ui.horizontal(|ui| {
                                if ui.button("Save").clicked() {
                                    action = Some((index, false));
                                }
                                if ui.button("Delete").clicked() {
                                    action = Some((index, true));
                                }
                            });
                            ui.end_row();
                        }
                    });
                });
            editor.focus_first = false;
            if let Some((index, delete)) = action {
                let result = if delete {
                    save_row(app, &editor.record, &editor.rows[index], true)
                } else {
                    resolved_row(&editor.rows[index], &editor.week)
                        .and_then(|row| save_row(app, &editor.record, &row, false))
                };
                match result {
                    Ok(saved) => {
                        editor.accept_saved(index, saved);
                        if let Some(row) = editor.rows.get_mut(index) {
                            if !delete && row.id != 0 {
                                row.start_date =
                                    crate::date_utils::preference(ui).display(&row.start_date);
                                row.end_date =
                                    crate::date_utils::preference(ui).display(&row.end_date);
                            }
                        }
                        editor.error.clear();
                        changed = true;
                    }
                    Err(error) => editor.error = error.to_string(),
                }
            }
            if ui.button("Add period").clicked() {
                editor.add_blank();
            }
            if !editor.error.is_empty() {
                ui.colored_label(ui.visuals().error_fg_color, &editor.error);
            }
            if ui.button("Close").clicked() {
                close = editor.request_close();
            }
        });
        if changed {
            if let Err(error) = self.refresh(app, records) {
                self.editor.as_mut().unwrap().error = error.to_string();
            }
        }
        if close {
            self.editor = None;
        }
    }
}

fn save_row(
    app: &Application,
    record: &PayrollTimesheet,
    row: &SicknessPeriod,
    delete: bool,
) -> Result<Option<SicknessPeriod>> {
    use crate::payroll_evidence::lifecycle::{stage, Stage};
    if stage(&crate::payroll_evidence::open(app)?, record)? != Stage::Editable {
        return Err("Submitted or settled payroll is protected from changes".into());
    }
    app.personal_assistant_repository
        .get_all()?
        .into_iter()
        .find(|pa| pa.id == record.personal_assistant_id)
        .ok_or("Personal Assistant not found")?;
    let repo = repository(app)?;
    if row.personal_assistant_id != record.personal_assistant_id {
        return Err("Sickness period belongs to another Personal Assistant".into());
    }
    if row.id != 0 {
        let stored = repo
            .get_by_id(row.id)?
            .ok_or("Sickness period no longer exists; reopen the editor")?;
        if stored.personal_assistant_id != record.personal_assistant_id {
            return Err("Sickness period belongs to another Personal Assistant".into());
        }
    }
    if delete {
        if row.id != 0 {
            repo.delete(row.id)?;
        }
        return Ok(None);
    }
    let id = if row.id == 0 {
        repo.insert(row.personal_assistant_id, &row.start_date, &row.end_date)?
    } else {
        repo.update(row.id, &row.start_date, &row.end_date)?;
        row.id
    };
    Ok(repo.get_by_id(id)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (
        tempfile::TempDir,
        Application,
        PayrollTimesheet,
        Vec<PayrollTimesheetWeek>,
    ) {
        let (dir, app) = crate::payroll_timesheet_screen::tests::test_application();
        crate::payroll_evidence::open(&app).unwrap().execute_batch(
            "INSERT INTO personal_assistants (id, first_name, surname) VALUES (1, 'Test', 'PA');
             INSERT INTO payroll_schedules (payroll_year, cycle_number, first_week_commencing, latest_posting_date, pay_date, created_at)
             VALUES ('2026/27', 7, '31/08/2026', 'test', 'test', 'test');"
        ).unwrap();
        let repo = &app.payroll_timesheet_repository;
        let id = repo.insert("2026/27", 7, 1, None, "test").unwrap();
        repo.insert_week(id, 1, "31/08/2026", 12.0).unwrap();
        repo.insert_week(id, 2, "07/09/2026", 15.0).unwrap();
        let record = repo.get_for_cycle_and_pa("2026/27", 7, 1).unwrap().unwrap();
        let weeks = repo.get_weeks(id).unwrap();
        (dir, app, record, weeks)
    }

    fn draft() -> SicknessPeriod {
        SicknessPeriod {
            id: 0,
            personal_assistant_id: 1,
            start_date: "5 Sep 2026".into(),
            end_date: "08/09/2026".into(),
        }
    }

    #[test]
    fn initial_empty_editor_has_blank_row_and_closes_without_warning() {
        let (_dir, _app, record, _) = setup();
        let mut editor = Editor::new(
            record,
            "12/10/2026".into(),
            "Test".into(),
            vec![],
            String::new(),
        );
        assert_eq!(editor.rows.len(), 1);
        assert!(editor.rows[0].start_date.is_empty() && editor.rows[0].end_date.is_empty());
        assert!(editor.focus_first);
        assert!(editor.request_close());
        editor.add_blank();
        assert_eq!(editor.rows.len(), 2);
    }

    #[test]
    fn contextual_days_support_month_boundaries_year_boundaries_and_full_dates() {
        let resolve = |week: &str, from: &str, to: &str| {
            let mut row = draft();
            row.start_date = from.into();
            row.end_date = to.into();
            let row = resolved_row(&row, week).unwrap();
            (row.start_date, row.end_date)
        };
        assert_eq!(
            resolve("12/10/2026", "15", "16"),
            ("2026-10-15".into(), "2026-10-16".into())
        );
        assert_eq!(
            resolve("26/10/2026", "30", "2"),
            ("2026-10-30".into(), "2026-11-02".into())
        );
        assert_eq!(
            resolve("28/12/2026", "31", "2"),
            ("2026-12-31".into(), "2027-01-02".into())
        );
        assert_eq!(
            resolve("12/10/2026", "01/09/2026", "2 Nov 2026"),
            ("2026-09-01".into(), "2026-11-02".into())
        );
        assert_eq!(
            resolve("12/10/2026", "2026-10-15", "16th October 2026"),
            ("2026-10-15".into(), "2026-10-16".into())
        );
        assert_eq!(
            resolve("26/02/2024", "29", "1"),
            ("2024-02-29".into(), "2024-03-01".into())
        );
    }

    #[test]
    fn invalid_or_unclear_days_require_full_dates_without_rolling_over() {
        for (week, input) in [
            ("12/10/2026", "0"),
            ("12/10/2026", "32"),
            ("12/04/2026", "31"),
            ("26/10/2026", "15"),
            ("12/10/2026", "31/02/2026"),
            ("12/10/2026", ""),
            ("12/10/2026", "abc"),
        ] {
            assert!(contextual_date(input, week).is_err(), "{week}: {input}");
        }
        let mut row = draft();
        row.start_date = "16".into();
        row.end_date = "15".into();
        assert!(resolved_row(&row, "12/10/2026").is_err());
    }

    #[test]
    fn close_confirmation_preserves_drafts_and_saved_rows_are_clean() {
        let (_dir, app, record, _) = setup();
        let mut editor = Editor::new(
            record,
            "26/10/2026".into(),
            "Test".into(),
            vec![],
            String::new(),
        );
        editor.rows[0].start_date = "30".into();
        assert!(!editor.request_close());
        assert!(editor.confirm_close);
        editor.confirm_close = false; // Cancel returns to the same draft.
        assert_eq!(editor.rows[0].start_date, "30");
        editor.rows[0].end_date = "2".into();
        let resolved = resolved_row(&editor.rows[0], &editor.week).unwrap();
        let saved = save_row(&app, &editor.record, &resolved, false).unwrap();
        editor.accept_saved(0, saved);
        assert!(editor.request_close());
        editor.rows[0].start_date = "30/10/2026".into();
        assert!(!editor.has_unsaved()); // Equivalent full-date display is not an edit.
        editor.rows[0].end_date = "3".into();
        assert!(!editor.request_close());
        let stored = repository(&app).unwrap().get_for_pa(1).unwrap();
        drop(editor); // Explicit discard closes without writing the modified draft.
        assert_eq!(repository(&app).unwrap().get_for_pa(1).unwrap(), stored);
        let mut reopened = Editor::new(
            stored_record(&app),
            "26/10/2026".into(),
            "Test".into(),
            stored,
            String::new(),
        );
        assert_eq!(reopened.rows.len(), 1);
        assert!(reopened.request_close());
        reopened.add_blank();
        reopened.rows[1].end_date = "bad".into();
        assert!(!reopened.request_close());
    }

    fn stored_record(app: &Application) -> PayrollTimesheet {
        app.payroll_timesheet_repository
            .get_for_cycle_and_pa("2026/27", 7, 1)
            .unwrap()
            .unwrap()
    }

    #[test]
    fn cross_week_edit_delete_and_revisit_share_one_record_and_refresh_markers() {
        let (_dir, app, record, weeks) = setup();
        let original_weeks = app
            .payroll_timesheet_repository
            .get_weeks(record.id)
            .unwrap();
        let saved = save_row(&app, &record, &draft(), false).unwrap().unwrap();
        assert_eq!(saved.start_date, "2026-09-05");
        let records = vec![(record.clone(), weeks.clone(), "Test PA".into())];
        let mut ui = SicknessUi::default();
        ui.refresh(&app, &records).unwrap();
        assert!(weeks.iter().all(|w| ui.has_data(w.id)));
        let mut from_second = week_periods(&app, 1, &weeks[1].week_commencing)
            .unwrap()
            .remove(0);
        assert_eq!(from_second.id, saved.id);
        from_second.end_date = "06/09/2026".into();
        save_row(&app, &record, &from_second, false).unwrap();
        let mut revisited = SicknessUi::default();
        revisited.refresh(&app, &records).unwrap();
        assert!(revisited.has_data(weeks[0].id));
        assert!(!revisited.has_data(weeks[1].id));
        assert_eq!(repository(&app).unwrap().get_for_pa(1).unwrap().len(), 1);
        save_row(&app, &record, &from_second, true).unwrap();
        revisited.refresh(&app, &records).unwrap();
        assert!(weeks.iter().all(|w| !revisited.has_data(w.id)));
        let current = app
            .payroll_timesheet_repository
            .get_weeks(record.id)
            .unwrap();
        for (before, after) in original_weeks.iter().zip(current) {
            assert_eq!(before.worked_hours, after.worked_hours);
            assert_eq!(before.sick_leave_hours, after.sick_leave_hours);
            assert_eq!(before.travel_miles, after.travel_miles);
        }
    }

    #[test]
    fn multiple_periods_validation_preserves_saved_data() {
        let (_dir, app, record, _) = setup();
        let saved = save_row(&app, &record, &draft(), false).unwrap().unwrap();
        let mut second = draft();
        second.start_date = "01/09/2026".into();
        second.end_date = "01/09/2026".into();
        save_row(&app, &record, &second, false).unwrap();
        assert_eq!(week_periods(&app, 1, "31/08/2026").unwrap().len(), 2);
        let mut bad = saved.clone();
        bad.end_date = "31/02/2026".into();
        assert!(save_row(&app, &record, &bad, false).is_err());
        assert_eq!(
            repository(&app).unwrap().get_by_id(saved.id).unwrap(),
            Some(saved.clone())
        );
        assert_eq!(repository(&app).unwrap().get_for_pa(1).unwrap().len(), 2);
    }

    #[test]
    fn legacy_flag_does_not_gate_sickness_crud_or_week_markers() {
        let (_dir, app, record, weeks) = setup();
        for legacy in [0, 1] {
            crate::payroll_evidence::open(&app)
                .unwrap()
                .execute(
                    "UPDATE personal_assistants SET sick_pay_enabled=?1 WHERE id=1",
                    [legacy],
                )
                .unwrap();
            let mut saved = save_row(&app, &record, &draft(), false).unwrap().unwrap();
            let mut ui = SicknessUi::default();
            ui.refresh(&app, &[(record.clone(), weeks.clone(), "Test PA".into())])
                .unwrap();
            assert!(weeks.iter().all(|week| ui.has_data(week.id)));
            saved.end_date = "09/09/2026".into();
            let edited = save_row(&app, &record, &saved, false).unwrap().unwrap();
            assert_eq!(edited.end_date, "2026-09-09");
            assert_eq!(
                week_periods(&app, 1, &weeks[1].week_commencing).unwrap()[0].id,
                saved.id
            );
            save_row(&app, &record, &saved, true).unwrap();
            assert!(repository(&app).unwrap().get_for_pa(1).unwrap().is_empty());
        }
    }

    #[test]
    fn protected_payroll_refuses_editor_writes_and_deletes() {
        let (_dir, app, record, _) = setup();
        let saved = save_row(&app, &record, &draft(), false).unwrap().unwrap();
        let db = crate::payroll_evidence::open(&app).unwrap();
        for sent in ["sent", "indeterminate:delivery"] {
            db.execute("INSERT OR REPLACE INTO payroll_timesheet_email_status (personal_assistant_id, payroll_year, cycle_number, email_type, sent_at) VALUES (1, '2026/27', 7, 'timesheet', ?1)", [sent]).unwrap();
            assert!(save_row(&app, &record, &draft(), false).is_err());
            assert!(save_row(&app, &record, &saved, false).is_err());
            assert!(save_row(&app, &record, &saved, true).is_err());
        }
        assert_eq!(
            repository(&app).unwrap().get_for_pa(1).unwrap(),
            vec![saved]
        );
    }
}
