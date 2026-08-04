#[derive(Debug)]
pub struct TimesheetEntry {
    pub id: i64,
    pub pa_name: String,
    pub start_time: String,
    pub end_time: String,
    pub break_minutes: i64,
    pub worked_minutes: i64,
    pub hourly_rate: f64,
    pub amount: f64,
    pub notes: Option<String>,
}
