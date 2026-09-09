//! Calendar dates only. Exact evidence timestamps deliberately do not use this layer.
use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DateDisplayFormat {
    Uk,
    #[default]
    ShortMonth,
    Iso,
}
impl DateDisplayFormat {
    pub const ALL: [Self; 3] = [Self::Uk, Self::ShortMonth, Self::Iso];
    pub fn label(self) -> &'static str {
        match self {
            Self::Uk => "DD/MM/YYYY — 09/09/2026",
            Self::ShortMonth => "D MMM YYYY — 9 Sep 2026",
            Self::Iso => "YYYY-MM-DD — 2026-09-09",
        }
    }
    pub fn format(self, date: NaiveDate) -> String {
        date.format(match self {
            Self::Uk => "%d/%m/%Y",
            Self::ShortMonth => "%-d %b %Y",
            Self::Iso => "%Y-%m-%d",
        })
        .to_string()
    }
    /// Rendering invalid legacy text is explicit, never a successful parse.
    pub fn display(self, value: &str) -> String {
        if value.trim().is_empty() {
            return String::new();
        }
        match parse_legacy(value) {
            Ok(date) => self.format(date),
            Err(_) => format!("Invalid date: {value}"),
        }
    }
}

pub fn parse_input(value: &str) -> Result<NaiveDate, String> {
    let value = value.trim();
    let tokens: Vec<String> = value
        .split_whitespace()
        .map(|token| {
            let lower = token.to_ascii_lowercase();
            for suffix in ["st", "nd", "rd", "th"] {
                if let Some(day) = lower.strip_suffix(suffix) {
                    if !day.is_empty() && day.len() <= 2 && day.bytes().all(|b| b.is_ascii_digit())
                    {
                        return day.to_string();
                    }
                }
            }
            token.to_string()
        })
        .collect();
    let normal = tokens.join(" ");
    let groups: Vec<&str> = normal
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .collect();
    let full_year = if normal.as_bytes().get(4) == Some(&b'-') {
        groups.first().is_some_and(|s| s.len() == 4)
    } else {
        groups.last().is_some_and(|s| s.len() == 4)
    };
    if full_year {
        for format in [
            "%d/%m/%Y", "%d-%m-%Y", "%Y-%m-%d", "%d %B %Y", "%d %b %Y", "%B %d %Y", "%b %d %Y",
        ] {
            if let Ok(date) = NaiveDate::parse_from_str(&normal, format) {
                if (1..=9999).contains(&date.year()) {
                    return Ok(date);
                }
            }
        }
    }
    Err(format!("Invalid calendar date '{value}'. Enter a real date with a four-digit year, for example 4 Apr 2026 or 04/04/2026 (day/month/year)."))
}

/// The existing chrono two-digit-year interpretation is retained only for stored data.
pub fn parse_legacy(value: &str) -> Result<NaiveDate, String> {
    parse_input(value).or_else(|error| {
        ["%d/%m/%y", "%d-%m-%y"]
            .iter()
            .find_map(|f| NaiveDate::parse_from_str(value.trim(), f).ok())
            .ok_or(error)
    })
}
pub fn uk(date: NaiveDate) -> String {
    DateDisplayFormat::Uk.format(date)
}
pub fn iso(date: NaiveDate) -> String {
    DateDisplayFormat::Iso.format(date)
}
pub fn formal(date: NaiveDate) -> String {
    date.format("%-d %B %Y").to_string()
}
pub fn compact(value: &str) -> Result<String, String> {
    parse_legacy(value).map(uk)
}
pub fn same(left: &str, right: &str) -> bool {
    left == right || matches!((parse_legacy(left), parse_legacy(right)), (Ok(a), Ok(b)) if a == b)
}
pub fn sql_error(error: String) -> rusqlite::Error {
    rusqlite::Error::InvalidParameterName(error)
}

/// Preserve untouched legacy values, including malformed text. Only edited fields
/// cross the strict input boundary; equivalent valid edits retain persisted bytes.
pub fn edited(value: &str, previous: Option<&str>, iso_storage: bool) -> rusqlite::Result<String> {
    if previous == Some(value) {
        return Ok(value.into());
    }
    let date = parse_input(value).map_err(sql_error)?;
    if previous.is_some_and(|old| parse_legacy(old).ok() == Some(date)) {
        return Ok(previous.unwrap().into());
    }
    Ok(if iso_storage { iso(date) } else { uk(date) })
}
pub fn optional_edited(
    value: Option<&str>,
    previous: Option<&str>,
    iso_storage: bool,
) -> rusqlite::Result<Option<String>> {
    if value == previous {
        return Ok(value.map(str::to_string));
    }
    value
        .filter(|s| !s.trim().is_empty())
        .map(|s| edited(s, previous, iso_storage))
        .transpose()
}

/// A separate presentation buffer prevents redraws/preferences from editing models.
pub fn edit(
    ui: &mut eframe::egui::Ui,
    value: &mut String,
    format: DateDisplayFormat,
) -> eframe::egui::Response {
    let id = ui.next_auto_id();
    let focused = ui.memory(|m| m.has_focus(id));
    let previous = ui
        .data_mut(|d| d.get_temp::<(String, String, bool)>(id))
        .filter(|(source, _, _)| source == value);
    let mut edited = previous.as_ref().is_some_and(|(_, _, edited)| *edited);
    let mut text = if let Some((_, text, _)) = previous.filter(|_| focused || edited) {
        text
    } else {
        parse_legacy(value)
            .map(|d| format.format(d))
            .unwrap_or_else(|_| value.clone())
    };
    let response = ui.add(
        eframe::egui::TextEdit::singleline(&mut text)
            .id(id)
            .desired_width(155.0)
            .hint_text("4 Apr 2026"),
    );
    if response.changed() {
        edited = true;
        *value = text.clone();
    }
    ui.data_mut(|d| d.insert_temp(id, (value.clone(), text, edited)));
    if !value.trim().is_empty()
        && (if edited {
            parse_input(value)
        } else {
            parse_legacy(value)
        })
        .is_err()
    {
        response.clone().on_hover_text(
            "Invalid stored date: correct this field using a real date and a four-digit year.",
        );
        ui.colored_label(ui.visuals().error_fg_color, "Invalid date");
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepted_full_year_inputs() {
        for value in [
            "04/04/2026",
            "4/4/2026",
            "4 April 2026",
            "4 Apr 2026",
            "4th April 2026",
            "4th Apr 2026",
            "April 4th 2026",
            "April 4 2026",
            "2026-04-04",
            "  4TH Apr 2026  ",
        ] {
            assert_eq!(iso(parse_input(value).unwrap()), "2026-04-04", "{value}");
        }
        for value in ["1st May 2026", "May 2ND 2026", "3rd May 2026"] {
            assert!(parse_input(value).is_ok());
        }
        assert_eq!(iso(parse_input("04/05/2026").unwrap()), "2026-05-04");
    }
    #[test]
    fn strict_year_and_calendar_validation() {
        for value in [
            "4/4/26",
            "4 Apr 26",
            "31/02/2026",
            "29/02/2025",
            "29/02/1900",
            "1/13/2026",
            "",
            "0/4/2026",
        ] {
            assert!(parse_input(value).is_err(), "{value}");
        }
        assert!(parse_input("29/02/2000").is_ok());
        assert!(parse_input("29/02/2024").is_ok());
        assert!(parse_legacy("4/4/26").is_ok());
        assert!(parse_legacy("4-4-26").is_ok());
    }
    #[test]
    fn independent_formats_and_semantic_equality() {
        let date = parse_input("09/09/2026").unwrap();
        assert_eq!(DateDisplayFormat::Uk.format(date), "09/09/2026");
        assert_eq!(DateDisplayFormat::default().format(date), "9 Sep 2026");
        assert_eq!(iso(date), "2026-09-09");
        assert_eq!(formal(date), "9 September 2026");
        assert_eq!(compact("9th Sep 2026").unwrap(), "09/09/2026");
        assert!(same("2026-09-09", "09/09/2026"));
        assert!(!same("bad", "worse"));
        assert_eq!(optional_edited(Some("  "), None, true).unwrap(), None);
        assert_eq!(edited("bad", Some("bad"), true).unwrap(), "bad");
        assert!(edited("new bad", Some("bad"), true).is_err());
        assert_eq!(
            edited("9 Sep 2026", Some("09/09/2026"), true).unwrap(),
            "09/09/2026"
        );
    }
}

pub fn set_display(ctx: &eframe::egui::Context, format: DateDisplayFormat) {
    ctx.data_mut(|data| {
        data.insert_temp(eframe::egui::Id::new("calendar_display_preference"), format)
    });
}
pub fn preference(ui: &eframe::egui::Ui) -> DateDisplayFormat {
    ui.data(|data| data.get_temp(eframe::egui::Id::new("calendar_display_preference")))
        .unwrap_or_default()
}
pub fn screen(ui: &eframe::egui::Ui, value: &str) -> String {
    preference(ui).display(value)
}

/// Format full numeric calendar dates in application-generated calendar prose.
/// Do not use this on raw evidence, timestamps, identifiers or user-authored text.
pub fn calendar_text(format: DateDisplayFormat, text: &str) -> String {
    let mut result = String::new();
    let mut pending = String::new();
    let flush = |pending: &mut String, result: &mut String| {
        if let Ok(date) = parse_input(pending) {
            result.push_str(&format.format(date));
        } else {
            result.push_str(pending);
        }
        pending.clear();
    };
    for ch in text.chars() {
        if ch.is_ascii_digit() || ch == '/' || ch == '-' {
            pending.push(ch);
        } else {
            flush(&mut pending, &mut result);
            result.push(ch);
        }
    }
    flush(&mut pending, &mut result);
    result
}
