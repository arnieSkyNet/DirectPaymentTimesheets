#[derive(Debug)]
pub struct TimesheetEntry {
    pub id: i64,
    pub pa_name: String,
    pub date: String,
    pub start_time: String,
    pub end_time: String,
    pub break_minutes: i32,
    pub notes: Option<String>,
}

