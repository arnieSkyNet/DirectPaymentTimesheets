use chrono::NaiveDate;

pub fn normalise_uk_date(input: &str) -> String {
    let formats = ["%d/%m/%Y", "%e/%m/%Y"];

    for format in formats {
        if let Ok(date) = NaiveDate::parse_from_str(input, format) {
            return date.format("%Y-%m-%d").to_string();
        }
    }

    input.to_string()
}

pub fn display_uk_date(input: &str) -> String {
    if let Ok(date) = NaiveDate::parse_from_str(input, "%Y-%m-%d") {
        return date.format("%d/%m/%Y").to_string();
    }

    input.to_string()
}
