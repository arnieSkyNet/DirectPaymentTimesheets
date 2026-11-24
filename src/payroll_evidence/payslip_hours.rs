//! Read a cycle-level working-hours total from explicitly itemised pay quantities.
use super::*;
use std::io::Read;

pub(super) enum Evidence {
    Missing,
    Unreadable,
    PaidMinutes(i64),
}

pub(super) fn paid_hours(
    app: &Application,
    db: &Connection,
    pa: i64,
    schedule: &crate::payroll_schedule_repository::PayrollSchedule,
) -> Evidence {
    match read_hours(app, db, pa, schedule) {
        Ok(evidence) => evidence,
        Err(_) => Evidence::Unreadable,
    }
}

fn read_hours(
    app: &Application,
    db: &Connection,
    pa: i64,
    schedule: &crate::payroll_schedule_repository::PayrollSchedule,
) -> Result<Evidence> {
    let registered = crate::payroll_replacement::current_payslip(
        db, pa, &schedule.payroll_year, schedule.cycle_number,
    )?;
    let (path, expected) = if let Some((path, digest)) = registered {
        (path, Some(digest))
    } else {
        let assistant = crate::personal_assistant_repository::PersonalAssistantRepository::get_by_id_on(db, pa)?;
        let root = crate::paths::expand_path(&app.context.config.folders.payslip_folder);
        let name = format!("{} {}", assistant.first_name.trim(), assistant.surname.trim());
        let path = crate::payroll_file_naming::existing_payslip_path(&root, &name, schedule)?;
        if !path.try_exists()? {
            return Ok(Evidence::Missing);
        }
        // Ordinary payslips found by name must still identify one PA.
        let assistants = app.personal_assistant_repository.get_all()?;
        let name = path.file_name().and_then(|name| name.to_str()).ok_or("Invalid payslip filename")?;
        if crate::archive::ordinary_payslip_owner(name, &assistants)? != Some(pa) {
            return Ok(Evidence::Unreadable);
        }
        (path, None)
    };
    let limit = crate::archive::MAX_PAYROLL_RETURN_ENTRY_BYTES;
    let mut bytes = Vec::new();
    crate::private_storage::open_read(&path)?.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit
        || expected.is_some_and(|digest| format!("{:x}", Sha256::digest(&bytes)) != digest)
    {
        return Ok(Evidence::Unreadable);
    }
    let text = pdf_extract::extract_text_from_mem(&bytes)?;
    Ok(parse(&text).map_or(Evidence::Unreadable, Evidence::PaidMinutes))
}

fn parse(text: &str) -> Option<i64> {
    let lines = text.lines().map(|line| {
        line.split_whitespace().collect::<Vec<_>>().join(" ").to_ascii_lowercase()
    }).collect::<Vec<_>>();
    let is_header = |line: &str| {
        matches!(line, "description hours rate amount" | "payment hours rate amount"
            | "payments hours rate amount" | "hours rate amount")
    };
    let mut starts = lines.iter().enumerate().filter(|(_, l)| is_header(l));
    let (start, _) = starts.next()?;
    if starts.next().is_some() {
        return None;
    }
    let mut rows = HashSet::new();
    let mut paid = 0i64;
    let mut found_total = false;
    for line in &lines[start + 1..] {
        if line.is_empty() {
            continue;
        }
        if line == "total payments" || line.starts_with("total payments ") {
            found_total = true;
            break;
        }
        if ["holiday", "ssp added"].iter().any(|label| line == label || line.starts_with(&format!("{label} "))) {
            continue;
        }
        let (label, values) = ["hourly rate", "pubhol x 1.5", "prevpay rate"]
            .iter()
            .find_map(|label| line.strip_prefix(label).and_then(|rest| rest.strip_prefix(' ')).map(|rest| (*label, rest)))?;
        let values = values.split_whitespace().collect::<Vec<_>>();
        let [hours, rate, amount] = values.as_slice() else { return None };
        let hours = number(hours)?;
        // The columns must also contain credible numeric rate and amount fields.
        let rate = number(rate.trim_start_matches('£'))?;
        let amount = number(amount.trim_start_matches('£'))?;
        if !rows.insert((label, hours.to_bits(), rate.to_bits(), amount.to_bits())) {
            return None;
        }
        if hours > 28.0 * 24.0 {
            return None;
        }
        paid = paid.checked_add((hours * 60.0).round() as i64)?;
    }
    (found_total && !rows.is_empty() && paid <= 28 * 24 * 60).then_some(paid)
}

fn number(token: &str) -> Option<f64> {
    if token.is_empty() || !token.bytes().all(|byte| byte.is_ascii_digit() || byte == b'.') {
        return None;
    }
    token.parse::<f64>().ok().filter(|value| value.is_finite())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paid_work_quantities_exclude_leave_ssp_and_money() {
        assert_eq!(parse("Description Hours Rate Amount\nHourly Rate 10 12.00 120.00\nPubHol X 1.5 2 18 36\nPrevPay Rate 3.25 11 35.75\nHoliday 4 12 48\nSSP added 35.00\nTotal Payments 274.75"), Some(915));
        assert_eq!(parse("Hourly Rate £120.00\nTotal Payments £150.50"), None);
        assert_eq!(parse("Description Hours Rate Amount\nHourly Rate 10 12 120\nHourly Rate 10 12 120\nTotal Payments 240"), None);
        assert_eq!(parse("Description Hours Rate Amount\nHourly Rate 0 12 0\nHoliday 4 12 48\nTotal Payments 48"), Some(0));
        assert_eq!(parse("Description Hours Rate Amount\nHourly Rate 10 12 120"), None);
    }
}
