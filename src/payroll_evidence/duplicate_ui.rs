use super::*;
use std::collections::BTreeMap;

#[derive(Default)]
pub struct DuplicateUi {
    pub pending: Vec<DuplicateGroup>,
    pub(crate) prepared: Option<Preflight>,
    selections: BTreeMap<String, String>,
    error: String,
    through: Option<NaiveDate>,
    reviews: BTreeMap<String, String>,
    acknowledged: bool,
}
impl DuplicateUi {
    pub fn refresh(&mut self, app: &Application, through: Option<NaiveDate>) -> Result<()> {
        self.through = through;
        let evidence = load(app)?;
        let preflight = preflight(&open(app)?, &evidence)?;
        self.pending = preflight
            .unresolved
            .iter()
            .cloned()
            .filter(|g| {
                through.is_none_or(|end| {
                    g.candidates
                        .iter()
                        .any(|e| e.date().is_ok_and(|d| d <= end))
                })
            })
            .collect();
        self.reviews.clear();
        self.acknowledged = false;
        let db = open(app)?;
        for g in &self.pending {
            if let Ok(review) = crate::shift_changes::group_review(&db, g) {
                self.reviews.insert(g.fingerprint.clone(), review.signature);
            }
        }
        self.prepared = Some(preflight);
        self.selections
            .retain(|k, _| self.pending.iter().any(|g| &g.fingerprint == k));
        Ok(())
    }
    pub fn show(&mut self, ui: &mut egui::Ui, app: &Application) -> bool {
        self.show_scoped(ui, app, None)
    }
    pub fn show_scoped(&mut self, ui: &mut egui::Ui, app: &Application, pa: Option<i64>) -> bool {
        let relevant =
            |g: &&DuplicateGroup| pa.is_none_or(|id| g.candidates.iter().any(|e| e.pa == id));
        if !self.pending.iter().any(|g| relevant(&g)) {
            return false;
        }
        ui.heading("Resolve possible duplicate shifts");
        ui.label("Review possible corrections and before/after alternatives");
        ui.label(
            "Original evidence is retained. Compare the alternatives and approve one payable version, retain legitimate separate shifts, or defer. Protected financial corrections still require the existing payroll review.",
        );
        egui::ScrollArea::vertical()
            .max_height(450.0)
            .show(ui, |ui| {
                for (index, g) in self.pending.iter().filter(relevant).enumerate() {
                    egui::Frame::group(ui.style())
                        .fill(if index % 2 == 0 {
                            ui.visuals().faint_bg_color
                        } else {
                            ui.visuals().extreme_bg_color
                        })
                        .show(ui, |ui| {
                            ui.strong(format!("Group {} — {}", index + 1, g.candidates[0].pa_name));
                            if ui.radio(self.selections.get(&g.fingerprint).is_some_and(|s|s=="separate"),"Retain both / all as legitimate separate shifts").clicked(){self.selections.insert(g.fingerprint.clone(),"separate".into());self.acknowledged=false;}
                            let exact=g.candidates.windows(2).all(|w|w[0].start==w[1].start && w[0].end==w[1].end && w[0].break_minutes==w[1].break_minutes && w[0].minutes==w[1].minutes);
                            ui.label(if exact {"Matching shift intervals and durations — possible duplicate records"}else{"Possible correction or separate overlapping/moved shifts — compare retained alternatives"});
                            if let Ok(db)=open(app){match crate::shift_changes::group_review(&db,g){Ok(review)=>{if review.protected{ui.label(format!("Protected payroll review: {}",review.description));}},Err(e)=>{ui.colored_label(ui.visuals().error_fg_color,e.to_string());}}}
                            ui.small("Choosing one version excludes the other alternatives from payable work, retaining every original. Retain all only if these are genuinely separate shifts.");
                            for e in &g.candidates {
                                let selected =
                                    self.selections.get(&g.fingerprint) == Some(&e.key());
                                if ui
                                    .radio(
                                        selected,
                                        format!(
                                            "{}: {} – {} | elapsed {:.2} minutes, worked {} minutes, break {} minutes",
                                            e.label(),
                                            e.start,
                                            e.end,
                                            (e.end_time().expect("group has exact interval")-e.start_time().expect("group has exact interval")).num_seconds() as f64/60.0,
                                            e.minutes,
                                            e.break_minutes
                                        ),
                                    )
                                    .clicked()
                                {
                                    self.selections.insert(g.fingerprint.clone(), e.key());
                                    self.acknowledged=false;
                                }
                                if e.source=="imported" {if let Ok(db)=open(app){if let Ok(Some(raw))=crate::repository::get_raw_on(&db,e.id){ui.small(format!("Imported rate £{:.2}/h; amount £{:.2} (retained source values)",raw.hourly_rate,raw.amount));}}}
                                if !e.notes.is_empty() {
                                    ui.label(&e.notes);
                                }
                                ui.small(format!("Evidence {}", e.key()));
                            }
                        });
                }
            });
        let complete = self.pending.iter().filter(relevant).all(|g| {
            self.selections.get(&g.fingerprint).is_some_and(|key| {
                key == "separate" || g.candidates.iter().any(|e| &e.key() == key)
            })
        });
        ui.checkbox(&mut self.acknowledged,"I approve these displayed choices and authorise affected protected-cycle source corrections");
        if ui
            .button("Defer review — continue unrelated work")
            .clicked()
        {
            let groups = self
                .pending
                .iter()
                .filter(relevant)
                .cloned()
                .collect::<Vec<_>>();
            match open(app).and_then(|db| crate::shift_changes::defer(&db, &groups)) {
                Ok(()) => {
                    self.pending.retain(|g| !relevant(&g));
                    self.prepared = None;
                    return false;
                }
                Err(e) => self.error = e.to_string(),
            }
        }
        if ui
            .add_enabled(
                complete && self.acknowledged,
                egui::Button::new("Approve reviewed choices"),
            )
            .clicked()
        {
            let result = (|| -> Result<()> {
                let choices = self
                    .pending
                    .iter()
                    .filter(relevant)
                    .map(|g| {
                        (
                            g.fingerprint.clone(),
                            self.selections[&g.fingerprint].clone(),
                        )
                    })
                    .collect::<Vec<_>>();
                resolve_reviewed(&open(app)?, &load(app)?, &choices, &self.reviews)
            })();
            match result {
                Ok(()) => {
                    self.pending.retain(|g| !relevant(&g));
                    self.selections
                        .retain(|key, _| self.pending.iter().any(|g| &g.fingerprint == key));
                    self.prepared = None;
                    self.error.clear();
                    return true;
                }
                Err(e) => {
                    self.error = e.to_string();
                    if let Err(refresh_error) = self.refresh(app, self.through) {
                        self.error.push_str(&format!("; {refresh_error}"));
                    }
                    // A failed choice can mean the evidence changed. Rebuild
                    // preparation before exposing any formerly blocked values.
                    return true;
                }
            }
        }
        if !self.error.is_empty() {
            ui.colored_label(ui.visuals().error_fg_color, &self.error);
        }
        false
    }
}

pub fn preparation_pa_scope(
    app: &Application,
    schedule: &crate::payroll_schedule_repository::PayrollSchedule,
) -> Result<HashSet<i64>> {
    let start = crate::models::parse_employment_date(&schedule.first_week_commencing)
        .ok_or("Invalid payroll date")?;
    let existing = app
        .payroll_timesheet_repository
        .get_all_for_cycle(&schedule.payroll_year, schedule.cycle_number)?
        .into_iter()
        .map(|r| r.personal_assistant_id)
        .collect::<HashSet<_>>();
    let mut ids = HashSet::new();
    for pa in app.personal_assistant_repository.get_all()? {
        if pa.eligible_for_period(
            start,
            start + chrono::Duration::days(27),
            existing.contains(&pa.id),
        )? {
            ids.insert(pa.id);
        }
    }
    Ok(ids)
}
