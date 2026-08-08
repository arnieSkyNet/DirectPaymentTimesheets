#[derive(Debug)]
pub struct TimesheetEntry {
    pub id: i64,
    pub pa_name: String,
    pub personal_assistant_id: Option<i64>,
    pub start_time: String,
    pub end_time: String,
    pub break_minutes: i64,
    pub worked_minutes: i64,
    pub hourly_rate: f64,
    pub amount: f64,
    pub notes: Option<String>,
}

#[derive(Debug)]
pub struct Employer {
    pub id: i64,
    pub name: String,

    pub date_of_birth: Option<String>,
    pub national_insurance_number: Option<String>,
    pub reference_account_number: Option<String>,

    pub address: Option<String>,
    pub telephone: Option<String>,
    pub email: Option<String>,

    pub employer_signature: Option<String>,
    pub default_pdf_template: Option<String>,
    pub sick_pay_enabled: bool,
    pub mileage_enabled: bool,
}

#[derive(Debug, Clone)]
pub struct PersonalAssistant {
    pub id: i64,
    pub first_name: String,
    pub surname: String,
    pub date_of_birth: Option<String>,
    pub national_insurance_number: Option<String>,
    pub address: Option<String>,
    pub postcode: Option<String>,
    pub telephone: Option<String>,
    pub email: Option<String>,
    pub employment_status: Option<String>,
    pub sick_pay_enabled: bool,
    pub mileage_enabled: bool,
    pub start_date: Option<String>,
    pub signature: Option<String>,
}
