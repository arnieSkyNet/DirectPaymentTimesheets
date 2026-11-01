//! Regional date suggestions only: never entitlement or automatic worked-hour subtraction.
use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::mpsc,
    time::Duration,
};

const URL: &str = "https://www.gov.uk/bank-holidays.json";
const MAX_BYTES: usize = 1024 * 1024;
const REFRESH_SECONDS: i64 = 7 * 24 * 3600;
const RETRY_SECONDS: i64 = 3600;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PublicHolidayRegion {
    #[default]
    #[serde(rename = "england-and-wales")]
    EnglandAndWales,
    #[serde(rename = "scotland")]
    Scotland,
    #[serde(rename = "northern-ireland")]
    NorthernIreland,
}
impl PublicHolidayRegion {
    pub const ALL: [Self; 3] = [Self::EnglandAndWales, Self::Scotland, Self::NorthernIreland];
    pub fn key(self) -> &'static str {
        match self {
            Self::EnglandAndWales => "england-and-wales",
            Self::Scotland => "scotland",
            Self::NorthernIreland => "northern-ireland",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::EnglandAndWales => "England and Wales",
            Self::Scotland => "Scotland",
            Self::NorthernIreland => "Northern Ireland",
        }
    }
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Division {
    division: String,
    events: Vec<Event>,
}
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Event {
    title: String,
    date: String,
    notes: String,
    bunting: bool,
}
type Feed = BTreeMap<String, Division>;
struct UniqueFeed(Feed);
impl<'de> Deserialize<'de> for UniqueFeed {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = UniqueFeed;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("unique regional divisions")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Self::Value, A::Error> {
                let mut feed = BTreeMap::new();
                while let Some((key, value)) = map.next_entry::<String, Division>()? {
                    if feed.len() >= 3 || feed.insert(key, value).is_some() {
                        return Err(serde::de::Error::custom("duplicate/unexpected division"));
                    }
                }
                Ok(UniqueFeed(feed))
            }
        }
        deserializer.deserialize_map(Visitor)
    }
}
fn parse(bytes: &[u8]) -> Result<Feed, String> {
    if bytes.len() > MAX_BYTES {
        return Err("Holiday response exceeds size limit".into());
    }
    let feed = serde_json::from_slice::<UniqueFeed>(bytes)
        .map_err(|_| "Invalid holiday JSON")?
        .0;
    if feed.len() != 3 {
        return Err("Unexpected holiday divisions".into());
    }
    for region in PublicHolidayRegion::ALL {
        let division = feed.get(region.key()).ok_or("Missing holiday division")?;
        if division.division != region.key()
            || division.events.is_empty()
            || division.events.len() > 2000
        {
            return Err("Invalid holiday division or event count".into());
        }
        let mut dates = BTreeMap::new();
        for event in &division.events {
            let date = NaiveDate::parse_from_str(&event.date, "%Y-%m-%d")
                .map_err(|_| "Invalid holiday date")?;
            if date.format("%Y-%m-%d").to_string() != event.date
                || !(1900..=2200).contains(&date.year())
            {
                return Err("Invalid holiday date format/range".into());
            }
            if event.title.trim().is_empty()
                || event.title.len() > 512
                || event.notes.len() > 1024
                || event
                    .title
                    .chars()
                    .chain(event.notes.chars())
                    .any(char::is_control)
            {
                return Err("Invalid holiday text".into());
            }
            if dates.insert(date, event).is_some() {
                return Err("Duplicate or conflicting holiday date".into());
            }
        }
    }
    Ok(feed)
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
#[derive(Serialize, Deserialize)]
struct Cache {
    format: u32,
    checked_at: i64,
    sha256: String,
    payload: String,
}
fn cache_path(root: &Path) -> PathBuf {
    root.join("bank-holidays-v1.json")
}
fn read_cache(root: &Path) -> Result<(Feed, i64), String> {
    let path = cache_path(root);
    if fs::metadata(&path)
        .map_err(|_| "No readable holiday cache")?
        .len()
        > (MAX_BYTES * 2) as u64
    {
        return Err("Holiday cache exceeds size limit".into());
    }
    let mut bytes = Vec::new();
    use std::io::Read;
    fs::File::open(path)
        .map_err(|_| "Cannot read holiday cache")?
        .take((MAX_BYTES * 2 + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "Cannot read holiday cache")?;
    if bytes.len() > MAX_BYTES * 2 {
        return Err("Holiday cache exceeds size limit".into());
    }
    let cache: Cache = serde_json::from_slice(&bytes).map_err(|_| "Invalid holiday cache")?;
    if cache.format != 1
        || cache.checked_at < 0
        || chrono::DateTime::from_timestamp(cache.checked_at, 0).is_none()
        || cache.sha256 != digest(cache.payload.as_bytes())
    {
        return Err("Holiday cache integrity check failed".into());
    }
    Ok((parse(cache.payload.as_bytes())?, cache.checked_at))
}
#[cfg(test)]
fn publish(root: &Path, bytes: &[u8], now: i64) -> Result<(), String> {
    publish_locked(root, bytes, now, &root.join("fixture.sqlite"))
}
fn publish_locked(root: &Path, bytes: &[u8], now: i64, database_path: &Path) -> Result<(), String> {
    parse(bytes)?;
    if now < 0 || chrono::DateTime::from_timestamp(now, 0).is_none() {
        return Err("Invalid holiday check timestamp".into());
    }
    // Cache replacement participates in the same lock as approval/publication/SMTP.
    // Download happens before this lock; busy dispatch simply retains the old cache.
    let _guard = crate::timesheet_delivery::production_lock_path(database_path).map_err(|_| {
        "Payroll dispatch/recovery active or lock unavailable; holiday refresh deferred"
    })?;
    let cache = Cache {
        format: 1,
        checked_at: now,
        sha256: digest(bytes),
        payload: String::from_utf8(bytes.to_vec()).map_err(|_| "Invalid holiday encoding")?,
    };
    let encoded = serde_json::to_vec(&cache).map_err(|_| "Cannot encode holiday cache")?;
    if encoded.len() > MAX_BYTES * 2 {
        return Err("Encoded holiday cache exceeds size limit".into());
    }
    let mut pending =
        tempfile::NamedTempFile::new_in(root).map_err(|_| "Cannot create holiday cache")?;
    pending
        .write_all(&encoded)
        .and_then(|_| pending.as_file().sync_all())
        .map_err(|_| "Cannot write holiday cache")?;
    pending
        .persist(cache_path(root))
        .map_err(|_| "Cannot publish holiday cache")?;
    Ok(())
}
fn download() -> Result<Vec<u8>, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .https_only(true)
        .max_redirects(0)
        .timeout_global(Some(Duration::from_secs(15)))
        .build()
        .into();
    let mut response = agent
        .get(URL)
        .header("Accept", "application/json")
        .call()
        .map_err(|_| "Holiday HTTPS request failed (connection, TLS or HTTP status)")?;
    if response.status() != 200 {
        return Err(
            "Holiday endpoint returned an unexpected status; redirects are not accepted".into(),
        );
    }
    response
        .body_mut()
        .with_config()
        .limit(MAX_BYTES as u64)
        .read_to_vec()
        .map_err(|_| "Cannot read bounded holiday response".into())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalendarBinding {
    pub region: PublicHolidayRegion,
    pub dates: Vec<NaiveDate>,
    pub sources: Vec<(i32, String)>,
}
#[derive(Debug)]
pub struct Calendar {
    pub binding: CalendarBinding,
    pub status: String,
    pub warnings: Vec<String>,
}
pub fn period(
    root: &Path,
    region: PublicHolidayRegion,
    start: NaiveDate,
    end: NaiveDate,
    now: i64,
) -> Calendar {
    let cache = read_cache(root);
    let mut dates = BTreeSet::new();
    let mut sources = Vec::new();
    let mut warnings = Vec::new();
    let mut coverage = BTreeSet::new();
    if let Ok((feed, _)) = &cache {
        coverage.extend(
            feed[region.key()]
                .events
                .iter()
                .filter_map(|e| NaiveDate::parse_from_str(&e.date, "%Y-%m-%d").ok())
                .map(|d| d.year()),
        );
    }
    for year in start.year()..=end.year() {
        if coverage.contains(&year) {
            let events = &cache.as_ref().unwrap().0[region.key()].events;
            let year_dates: Vec<_> = events
                .iter()
                .filter_map(|e| NaiveDate::parse_from_str(&e.date, "%Y-%m-%d").ok())
                .filter(|d| d.year() == year)
                .collect();
            if year_dates.len()
                < match region {
                    PublicHolidayRegion::EnglandAndWales => 8,
                    PublicHolidayRegion::Scotland => 9,
                    PublicHolidayRegion::NorthernIreland => 10,
                }
                || !year_dates.iter().any(|d| d.month() == 12)
                || !year_dates.iter().any(|d| d.month() == 1 && d.day() <= 7)
            {
                warnings.push(format!(
                    "{year}: published coverage appears incomplete; review manually."
                ));
            }
            dates.extend(year_dates);
            sources.push((year, "GOV.UK published feed".into()));
        } else if region == PublicHolidayRegion::EnglandAndWales {
            dates.extend(bank_holidays_for_period(
                NaiveDate::from_ymd_opt(year, 1, 1).unwrap(),
                NaiveDate::from_ymd_opt(year, 12, 31).unwrap(),
            ));
            sources.push((year, "Recurring England/Wales fallback".into()));
            warnings.push(format!("{year}: incomplete recurring fallback; exceptional and moved holidays require manual review."));
        } else {
            sources.push((year, "Unavailable".into()));
            warnings.push(format!("{year}: regional data unavailable; manual review required. No England/Wales dates substituted."));
        }
    }
    let status = match cache {
        Ok((_, checked)) => format!("{} — GOV.UK cache checked {} ({}). Published years: {:?}; completeness is not guaranteed.", region.label(), chrono::DateTime::from_timestamp(checked, 0).map(|d|d.to_rfc3339()).unwrap_or_else(||"invalid timestamp".into()), if now < checked || now-checked >= REFRESH_SECONDS { "stale or clock changed" } else { "recent" }, coverage),
        Err(error) => format!("{} — {error}. Offline date suggestions only.", region.label()),
    };
    Calendar {
        binding: CalendarBinding {
            region,
            dates: dates
                .into_iter()
                .filter(|d| *d >= start && *d <= end)
                .collect(),
            sources,
        },
        status,
        warnings,
    }
}
pub fn for_application(app: &crate::app::Application, start: NaiveDate) -> Calendar {
    period(
        &app.context.environment.cache_dir,
        app.context.config.payroll.public_holiday_region,
        start,
        start + chrono::Duration::days(27),
        chrono::Utc::now().timestamp(),
    )
}
/// Reload persisted configuration and cache, including changes made by another instance.
pub fn verify(
    app: &crate::app::Application,
    start: NaiveDate,
    approved: &CalendarBinding,
) -> Result<(), Box<dyn std::error::Error>> {
    let config_path = app.context.environment.data_dir.join("config.toml");
    let region = match fs::read_to_string(&config_path) {
        Ok(contents) => {
            toml::from_str::<crate::config::AppConfig>(&contents)
                .map_err(|_| "Invalid persisted holiday configuration; review settings")?
                .payroll
                .public_holiday_region
        }
        Err(error)
            if error.kind() == std::io::ErrorKind::NotFound
                && fs::symlink_metadata(&config_path).is_err() =>
        {
            app.context.config.payroll.public_holiday_region
        }
        Err(_) => return Err("Cannot verify persisted holiday configuration".into()),
    };
    let current = period(
        &app.context.environment.cache_dir,
        region,
        start,
        start + chrono::Duration::days(27),
        chrono::Utc::now().timestamp(),
    )
    .binding;
    if current != *approved || region != app.context.config.payroll.public_holiday_region {
        return Err("Public-holiday region or relevant calendar dates/source changed; reload preparation and approve again".into());
    }
    Ok(())
}
pub fn comparison(
    binding: &CalendarBinding,
    stored: &[crate::payroll_timesheet_repository::PayrollTimesheetPublicHoliday],
) -> Vec<String> {
    let actual: BTreeSet<_> = stored
        .iter()
        .filter_map(|h| crate::date_utils::parse_legacy(&h.holiday_date).ok())
        .collect();
    let expected: BTreeSet<_> = binding.dates.iter().copied().collect();
    actual.difference(&expected).map(|d|format!("Stored holiday {d} is absent from the selected calendar; its hours are retained.")).chain(expected.difference(&actual).map(|d|format!("Calendar holiday {d} is absent from this draft; existing rows are unchanged. Review manually."))).collect()
}
#[derive(Default)]
pub struct Refresh {
    pending: Option<mpsc::Receiver<Result<(), String>>>,
    next_attempt: i64,
    next_check: i64,
    pub status: Option<String>,
}
impl Refresh {
    pub fn running(&self) -> bool {
        self.pending.is_some()
    }
    pub fn tick(
        &mut self,
        root: &Path,
        database: &Path,
        now: i64,
        ctx: &egui::Context,
        manual: bool,
    ) {
        if let Some(rx) = &self.pending {
            match rx.try_recv() {
                Ok(result) => {
                    self.status = Some(
                        result
                            .map(|_| {
                                "Holiday cache refreshed; existing payroll is unchanged".into()
                            })
                            .unwrap_or_else(|e| format!("{e}; last valid cache retained")),
                    );
                    self.pending = None;
                    self.next_attempt = now + RETRY_SECONDS;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.pending = None;
                    self.next_attempt = now + RETRY_SECONDS;
                    self.status =
                        Some("Holiday refresh interrupted; last valid cache retained".into());
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if self.running() || (!manual && now < self.next_check) {
            return;
        }
        self.next_check = now + 60;
        let due = read_cache(root)
            .map(|(_, checked)| now < checked || now - checked >= REFRESH_SECONDS)
            .unwrap_or(true);
        if !self.running() && (manual || (due && now >= self.next_attempt)) {
            self.start(
                root.to_owned(),
                database.to_owned(),
                now,
                ctx.clone(),
                download,
            );
        }
    }
    fn start<F>(&mut self, root: PathBuf, database: PathBuf, now: i64, ctx: egui::Context, fetch: F)
    where
        F: FnOnce() -> Result<Vec<u8>, String> + Send + 'static,
    {
        let (tx, rx) = mpsc::channel();
        self.pending = Some(rx);
        self.status = Some("Refreshing GOV.UK holiday data…".into());
        std::thread::spawn(move || {
            let result = fetch().and_then(|bytes| publish_locked(&root, &bytes, now, &database));
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }
}
fn bank_holidays_for_period(
    start_date: chrono::NaiveDate,
    end_date: chrono::NaiveDate,
) -> Vec<chrono::NaiveDate> {
    let mut holidays = Vec::new();

    for year in start_date.year()..=end_date.year() {
        holidays.extend(bank_holidays_for_year(year));
    }

    holidays.retain(|date| *date >= start_date && *date <= end_date);

    holidays.sort();

    holidays.dedup();

    holidays
}

fn bank_holidays_for_year(year: i32) -> Vec<chrono::NaiveDate> {
    use chrono::Weekday;

    let mut holidays = Vec::new();

    // New Year's Day.
    let new_year = chrono::NaiveDate::from_ymd_opt(year, 1, 1).expect("Invalid New Year's Day");

    holidays.push(observed_monday_if_weekend(new_year));

    // Easter-related bank holidays.
    let easter_sunday = calculate_easter_sunday(year);

    holidays.push(easter_sunday - chrono::Duration::days(2)); // Good Friday
    holidays.push(easter_sunday + chrono::Duration::days(1)); // Easter Monday

    // Early May bank holiday - first Monday in May.
    holidays.push(first_monday_of_month(year, 5));

    // Spring bank holiday - last Monday in May.
    holidays.push(last_monday_of_month(year, 5));

    // Summer bank holiday - last Monday in August.
    holidays.push(last_monday_of_month(year, 8));

    // Christmas Day and Boxing Day.
    //
    // Where either falls on a weekend, use the published
    // substitute-day arrangement.
    let christmas = chrono::NaiveDate::from_ymd_opt(year, 12, 25).expect("Invalid Christmas Day");

    let boxing_day = chrono::NaiveDate::from_ymd_opt(year, 12, 26).expect("Invalid Boxing Day");

    match (christmas.weekday(), boxing_day.weekday()) {
        (Weekday::Sat, Weekday::Sun) => {
            holidays.push(
                chrono::NaiveDate::from_ymd_opt(year, 12, 27)
                    .expect("Invalid Christmas substitute day"),
            );

            holidays.push(
                chrono::NaiveDate::from_ymd_opt(year, 12, 28)
                    .expect("Invalid Boxing Day substitute day"),
            );
        }

        (Weekday::Sun, Weekday::Mon) => {
            holidays.push(boxing_day);

            holidays.push(
                chrono::NaiveDate::from_ymd_opt(year, 12, 27)
                    .expect("Invalid Christmas substitute day"),
            );
        }

        (_, Weekday::Sat) => {
            holidays.push(christmas);

            holidays.push(boxing_day + chrono::Duration::days(2));
        }

        _ => {
            holidays.push(christmas);
            holidays.push(boxing_day);
        }
    }

    holidays
}

fn observed_monday_if_weekend(date: chrono::NaiveDate) -> chrono::NaiveDate {
    match date.weekday() {
        chrono::Weekday::Sat => date + chrono::Duration::days(2),
        chrono::Weekday::Sun => date + chrono::Duration::days(1),
        _ => date,
    }
}

fn first_monday_of_month(year: i32, month: u32) -> chrono::NaiveDate {
    let first =
        chrono::NaiveDate::from_ymd_opt(year, month, 1).expect("Invalid first day of month");

    first + chrono::Duration::days((7 - first.weekday().num_days_from_monday()) as i64 % 7)
}

fn last_monday_of_month(year: i32, month: u32) -> chrono::NaiveDate {
    let next_month = if month == 12 {
        chrono::NaiveDate::from_ymd_opt(year + 1, 1, 1).expect("Invalid next year")
    } else {
        chrono::NaiveDate::from_ymd_opt(year, month + 1, 1).expect("Invalid next month")
    };

    let last_day = next_month - chrono::Duration::days(1);

    last_day - chrono::Duration::days(last_day.weekday().num_days_from_monday() as i64)
}

fn calculate_easter_sunday(year: i32) -> chrono::NaiveDate {
    let a = year % 19;
    let b = year / 100;
    let c = year % 100;
    let d = b / 4;
    let e = b % 4;
    let f = (b + 8) / 25;
    let g = (b - f + 1) / 3;
    let h = (19 * a + b - d - g + 15) % 30;
    let i = c / 4;
    let k = c % 4;
    let l = (32 + 2 * e + 2 * i - h - k) % 7;
    let m = (a + 11 * h + 22 * l) / 451;

    let month = (h + l - 7 * m + 114) / 31;
    let day = ((h + l - 7 * m + 114) % 31) + 1;

    chrono::NaiveDate::from_ymd_opt(year, month as u32, day as u32)
        .expect("Invalid calculated Easter Sunday")
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    fn date(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }
    pub(crate) fn fixture() -> Vec<u8> {
        let mut feed = serde_json::Map::new();
        for region in PublicHolidayRegion::ALL {
            let dates = match region {
                PublicHolidayRegion::EnglandAndWales => vec![
                    "2026-01-01",
                    "2026-04-03",
                    "2026-04-06",
                    "2026-05-04",
                    "2026-05-25",
                    "2026-08-31",
                    "2026-12-25",
                    "2026-12-28",
                    "2027-01-01",
                ],
                PublicHolidayRegion::Scotland => vec![
                    "2026-01-01",
                    "2026-01-02",
                    "2026-04-03",
                    "2026-05-04",
                    "2026-05-25",
                    "2026-08-03",
                    "2026-11-30",
                    "2026-12-25",
                    "2026-12-28",
                    "2027-01-04",
                ],
                PublicHolidayRegion::NorthernIreland => vec![
                    "2026-01-01",
                    "2026-03-17",
                    "2026-04-03",
                    "2026-04-06",
                    "2026-05-04",
                    "2026-05-25",
                    "2026-07-13",
                    "2026-08-31",
                    "2026-12-25",
                    "2026-12-28",
                    "2027-01-01",
                ],
            };
            let events: Vec<_> = dates.iter().map(|date| serde_json::json!({"title":"Synthetic published holiday", "date":date, "notes":"Substitute or exceptional dates are already published", "bunting":false})).collect();
            feed.insert(
                region.key().into(),
                serde_json::json!({"division":region.key(),"events":events}),
            );
        }
        serde_json::to_vec(&feed).unwrap()
    }
    pub(crate) fn seed(root: &Path, now: i64) {
        fs::create_dir_all(root).unwrap();
        publish(root, &fixture(), now).unwrap();
    }
    #[test]
    fn configuration_defaults_round_trips_and_rejects_unknown_region() {
        let mut value = toml::Value::try_from(crate::config::AppConfig::default()).unwrap();
        value["payroll"]
            .as_table_mut()
            .unwrap()
            .remove("public_holiday_region");
        let config: crate::config::AppConfig = value.clone().try_into().unwrap();
        assert_eq!(
            config.payroll.public_holiday_region,
            PublicHolidayRegion::EnglandAndWales
        );
        for region in PublicHolidayRegion::ALL {
            value["payroll"]
                .as_table_mut()
                .unwrap()
                .insert("public_holiday_region".into(), region.key().into());
            let config: crate::config::AppConfig = value.clone().try_into().unwrap();
            assert_eq!(config.payroll.public_holiday_region, region);
            assert!(toml::to_string(&config).unwrap().contains(region.key()));
        }
        value["payroll"]["public_holiday_region"] = "unknown-region".into();
        assert!(value.try_into::<crate::config::AppConfig>().is_err());
    }
    #[test]
    fn regional_dates_and_cross_year_periods_use_published_dates_directly() {
        let root = tempfile::tempdir().unwrap();
        seed(root.path(), 100);
        let start = date("2026-12-20");
        let end = start + chrono::Duration::days(27);
        let ew = period(
            root.path(),
            PublicHolidayRegion::EnglandAndWales,
            start,
            end,
            100,
        );
        let sc = period(root.path(), PublicHolidayRegion::Scotland, start, end, 100);
        assert!(ew.binding.dates.contains(&date("2027-01-01")));
        assert!(!sc.binding.dates.contains(&date("2027-01-01")));
        assert!(sc.binding.dates.contains(&date("2027-01-04")));
        assert!(sc.binding.dates.contains(&date("2026-12-28")));
        assert!(sc.warnings.iter().any(|w| w.contains("incomplete")));
        let ni = period(
            root.path(),
            PublicHolidayRegion::NorthernIreland,
            date("2026-07-01"),
            date("2026-07-28"),
            100,
        );
        assert_eq!(ni.binding.dates, vec![date("2026-07-13")]);
    }
    #[test]
    fn exceptional_and_moved_dates_are_never_merged_with_recurring_rules() {
        let root = tempfile::tempdir().unwrap();
        let mut fixture: serde_json::Value = serde_json::from_slice(&fixture()).unwrap();
        fixture["england-and-wales"]["events"][3]["date"] = "2026-05-08".into();
        fixture["england-and-wales"]["events"].as_array_mut().unwrap().push(serde_json::json!({"title":"Synthetic exception", "date":"2026-05-11", "notes":"", "bunting":false}));
        publish(root.path(), &serde_json::to_vec(&fixture).unwrap(), 100).unwrap();
        let calendar = period(
            root.path(),
            PublicHolidayRegion::EnglandAndWales,
            date("2026-05-01"),
            date("2026-05-28"),
            100,
        );
        assert!(calendar.binding.dates.contains(&date("2026-05-08")));
        assert!(calendar.binding.dates.contains(&date("2026-05-11")));
        assert!(!calendar.binding.dates.contains(&date("2026-05-04")));
    }
    #[test]
    fn unavailable_regional_years_never_use_england_wales_fallback() {
        let root = tempfile::tempdir().unwrap();
        for region in [
            PublicHolidayRegion::Scotland,
            PublicHolidayRegion::NorthernIreland,
        ] {
            let calendar = period(
                root.path(),
                region,
                date("2030-01-01"),
                date("2030-01-28"),
                100,
            );
            assert!(calendar.binding.dates.is_empty());
            assert!(calendar.warnings[0].contains("unavailable"));
        }
        let ew = period(
            root.path(),
            PublicHolidayRegion::EnglandAndWales,
            date("2030-01-01"),
            date("2030-01-28"),
            100,
        );
        assert_eq!(ew.binding.dates, vec![date("2030-01-01")]);
        assert!(ew.warnings[0].contains("incomplete recurring"));
    }
    #[test]
    fn malformed_oversized_duplicate_and_conflicting_responses_are_rejected() {
        assert!(parse(&vec![b' '; MAX_BYTES + 1]).is_err());
        assert!(parse(b"not JSON").is_err());
        for mutate in 0..7 {
            let mut value: serde_json::Value = serde_json::from_slice(&fixture()).unwrap();
            match mutate {
                0 => value["scotland"]["division"] = "england-and-wales".into(),
                1 => value["scotland"]["events"][0]["date"] = "2026-02-30".into(),
                2 => value["scotland"]["events"][0]["date"] = "2026-1-01".into(),
                3 => value["scotland"]["events"][0]["title"] = "x".repeat(513).into(),
                4 => {
                    let event = value["scotland"]["events"][0].clone();
                    value["scotland"]["events"]
                        .as_array_mut()
                        .unwrap()
                        .push(event);
                }
                5 => value["scotland"]["events"] = serde_json::json!([]),
                _ => {
                    value.as_object_mut().unwrap().remove("scotland");
                }
            }
            assert!(parse(&serde_json::to_vec(&value).unwrap()).is_err());
        }
        let text = String::from_utf8(fixture()).unwrap();
        let duplicate = format!(
            "{{\"scotland\":{},{}",
            serde_json::from_str::<serde_json::Value>(&text).unwrap()["scotland"],
            &text[1..]
        );
        assert!(parse(duplicate.as_bytes()).is_err());
    }
    #[test]
    fn cache_corruption_and_failed_or_interrupted_replacements_retain_evidence() {
        let root = tempfile::tempdir().unwrap();
        seed(root.path(), 100);
        let original = fs::read(cache_path(root.path())).unwrap();
        assert!(publish(root.path(), b"invalid", 200).is_err());
        assert_eq!(fs::read(cache_path(root.path())).unwrap(), original);
        fs::write(root.path().join("unfinished-temporary-cache"), b"partial").unwrap();
        assert!(read_cache(root.path()).is_ok());
        let mut cache: Cache = serde_json::from_slice(&original).unwrap();
        cache.payload.push(' ');
        fs::write(cache_path(root.path()), serde_json::to_vec(&cache).unwrap()).unwrap();
        assert!(read_cache(root.path()).is_err());
    }
    #[test]
    fn refresh_timestamp_and_unrelated_dates_do_not_change_period_approval() {
        let root = tempfile::tempdir().unwrap();
        seed(root.path(), 100);
        let binding = period(
            root.path(),
            PublicHolidayRegion::Scotland,
            date("2026-04-01"),
            date("2026-04-28"),
            100,
        )
        .binding;
        seed(root.path(), 200);
        assert_eq!(
            binding,
            period(
                root.path(),
                PublicHolidayRegion::Scotland,
                date("2026-04-01"),
                date("2026-04-28"),
                200
            )
            .binding
        );
        let mut value: serde_json::Value = serde_json::from_slice(&fixture()).unwrap();
        value["scotland"]["events"][0]["title"] = "Renamed title".into();
        publish(root.path(), &serde_json::to_vec(&value).unwrap(), 300).unwrap();
        assert_eq!(
            binding,
            period(
                root.path(),
                PublicHolidayRegion::Scotland,
                date("2026-04-01"),
                date("2026-04-28"),
                300
            )
            .binding
        );
    }
    #[test]
    fn background_refresh_is_nonblocking_and_failures_keep_last_good_cache() {
        let root = tempfile::tempdir().unwrap();
        seed(root.path(), 100);
        let original = fs::read(cache_path(root.path())).unwrap();
        let (release, wait) = mpsc::channel();
        let (finished, done) = mpsc::channel();
        let ctx = egui::Context::default();
        let mut refresh = Refresh::default();
        refresh.start(
            root.path().into(),
            root.path().join("fixture.sqlite"),
            200,
            ctx.clone(),
            move || {
                wait.recv().unwrap();
                finished.send(()).unwrap();
                Err("Synthetic TLS verification failure".into())
            },
        );
        assert!(refresh.running());
        assert_eq!(fs::read(cache_path(root.path())).unwrap(), original);
        release.send(()).unwrap();
        done.recv_timeout(Duration::from_secs(5)).unwrap();
        let result = refresh
            .pending
            .as_ref()
            .unwrap()
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        let (tx, rx) = mpsc::channel();
        tx.send(result).unwrap();
        refresh.pending = Some(rx);
        refresh.tick(
            root.path(),
            &root.path().join("fixture.sqlite"),
            200,
            &ctx,
            false,
        );
        assert!(!refresh.running());
        assert!(refresh.status.as_ref().unwrap().contains("TLS"));
        assert_eq!(fs::read(cache_path(root.path())).unwrap(), original);
        assert_eq!(refresh.next_attempt, 200 + RETRY_SECONDS);
    }
    #[test]
    fn comparison_preserves_stored_rows_and_hours_without_mixed_region_insertion() {
        let stored = vec![
            crate::payroll_timesheet_repository::PayrollTimesheetPublicHoliday {
                id: 1,
                payroll_timesheet_id: 1,
                week_number: 1,
                holiday_date: "01/01/2027".into(),
                hours: 7.0,
            },
        ];
        let binding = CalendarBinding {
            region: PublicHolidayRegion::Scotland,
            dates: vec![date("2027-01-04")],
            sources: vec![(2027, "feed".into())],
        };
        let warnings = comparison(&binding, &stored);
        assert_eq!(warnings.len(), 2);
        assert!(warnings[0].contains("2027-01-01"));
        assert!(warnings[1].contains("2027-01-04"));
        assert_eq!(stored[0].hours, 7.0);
    }
    #[test]
    fn cache_publication_is_blocked_by_active_dispatch_and_retains_last_good_data() {
        let directory = tempfile::tempdir().unwrap();
        let cache = directory.path().join("cache");
        let root = &cache;
        seed(root, 100);
        let original = fs::read(cache_path(root)).unwrap();
        let db = crate::database::open(&directory.path().join("database.sqlite")).unwrap();
        let guard = crate::timesheet_delivery::production_lock(&db).unwrap();
        assert!(publish_locked(
            root,
            &fixture(),
            200,
            &directory.path().join("database.sqlite")
        )
        .is_err());
        assert_eq!(fs::read(cache_path(root)).unwrap(), original);
        drop(guard);
        publish_locked(
            root,
            &fixture(),
            200,
            &directory.path().join("database.sqlite"),
        )
        .unwrap();
        assert_eq!(read_cache(root).unwrap().1, 200);
    }

    #[test]
    fn cache_publication_shares_lock_during_database_replacement_without_creating_database() {
        let directory = tempfile::tempdir().unwrap();
        let cache = directory.path().join("cache");
        seed(&cache, 100);
        let database = directory.path().join("database.sqlite");
        let db = crate::database::open(&database).unwrap();
        let guard = crate::timesheet_delivery::production_lock(&db).unwrap();
        drop(db);
        fs::rename(&database, directory.path().join("original.sqlite")).unwrap();
        assert!(publish_locked(&cache, &fixture(), 200, &database).is_err());
        assert!(!database.exists());
        assert_eq!(read_cache(&cache).unwrap().1, 100);
        drop(guard);
        publish_locked(&cache, &fixture(), 200, &database).unwrap();
        assert!(!database.exists());
        assert!(publish_locked(
            &cache,
            &fixture(),
            300,
            &directory.path().join("missing-parent/database.sqlite")
        )
        .is_err());
        assert_eq!(read_cache(&cache).unwrap().1, 200);
    }

    #[test]
    fn cache_failure_and_future_clock_show_warnings_without_losing_regional_dates() {
        let root = tempfile::tempdir().unwrap();
        seed(root.path(), 100);
        let recent = period(
            root.path(),
            PublicHolidayRegion::Scotland,
            date("2026-01-01"),
            date("2026-01-28"),
            100,
        );
        for now in [50, 100 + REFRESH_SECONDS] {
            let stale = period(
                root.path(),
                PublicHolidayRegion::Scotland,
                date("2026-01-01"),
                date("2026-01-28"),
                now,
            );
            assert!(stale.status.contains("stale or clock changed"));
            assert_eq!(stale.binding, recent.binding);
        }
        let mut refresh = Refresh::default();
        let ctx = egui::Context::default();
        refresh.start(
            root.path().into(),
            root.path().join("fixture.sqlite"),
            200,
            ctx.clone(),
            || Err("Synthetic offline failure".into()),
        );
        let result = refresh
            .pending
            .as_ref()
            .unwrap()
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        let (tx, rx) = mpsc::channel();
        tx.send(result).unwrap();
        refresh.pending = Some(rx);
        refresh.tick(
            root.path(),
            &root.path().join("fixture.sqlite"),
            200,
            &ctx,
            false,
        );
        assert!(refresh.status.unwrap().contains("offline"));
        assert_eq!(
            period(
                root.path(),
                PublicHolidayRegion::Scotland,
                date("2026-01-01"),
                date("2026-01-28"),
                200
            )
            .binding,
            recent.binding
        );
    }

    #[test]
    fn https_client_rejects_insecure_endpoints_and_redirects() {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .https_only(true)
            .max_redirects(0)
            .timeout_global(Some(Duration::from_secs(15)))
            .build()
            .into();
        assert!(agent.get("http://127.0.0.1:1/").call().is_err());
        assert!(agent.config().https_only());
        assert_eq!(agent.config().max_redirects(), 0);
    }
}
