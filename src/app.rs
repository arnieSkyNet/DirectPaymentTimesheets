use rusqlite::Connection;

use crate::context::AppContext;
use crate::contracted_hours_repository::ContractedHoursRepository;
use crate::employer_repository::EmployerRepository;
use crate::import_service::ImportService;
use crate::pay_rate_repository::PayRateRepository;
use crate::payroll_prep_sheet_import_service::PayrollPrepSheetImportService;
use crate::payroll_provider_repository::PayrollProviderRepository;
use crate::payroll_schedule_repository::PayrollScheduleRepository;
use crate::payroll_timesheet_repository::PayrollTimesheetRepository;
use crate::pdf_generator::{PdfGenerator, TimesheetPdfData};
use crate::personal_assistant_repository::PersonalAssistantRepository;
use crate::repository::TimesheetRepository;

pub struct Application {
    pub context: AppContext,
    pub repository: TimesheetRepository,
    pub employer_repository: EmployerRepository,
    pub personal_assistant_repository: PersonalAssistantRepository,
    pub pay_rate_repository: PayRateRepository,
    pub contracted_hours_repository: ContractedHoursRepository,
    pub payroll_provider_repository: PayrollProviderRepository,
    pub payroll_schedule_repository: PayrollScheduleRepository,
    pub payroll_timesheet_repository: PayrollTimesheetRepository,
}

impl Application {
    pub fn initialise() -> Result<Self, Box<dyn std::error::Error>> {
        let context = AppContext::initialise()?;

        let database_path = &context.environment.database_path;

        let repository = TimesheetRepository::new(Connection::open(database_path)?);

        let employer_repository = EmployerRepository::new(Connection::open(database_path)?);

        let personal_assistant_repository =
            PersonalAssistantRepository::new(Connection::open(database_path)?);

        let pay_rate_repository = PayRateRepository::new(Connection::open(database_path)?);

        let contracted_hours_repository =
            ContractedHoursRepository::new(Connection::open(database_path)?);

        let payroll_provider_repository =
            PayrollProviderRepository::new(Connection::open(database_path)?);

        let payroll_schedule_repository =
            PayrollScheduleRepository::new(Connection::open(database_path)?);

        let payroll_timesheet_repository =
            PayrollTimesheetRepository::new(Connection::open(database_path)?);

        Ok(Self {
            context,
            repository,
            employer_repository,
            personal_assistant_repository,
            pay_rate_repository,
            contracted_hours_repository,
            payroll_provider_repository,
            payroll_schedule_repository,
            payroll_timesheet_repository,
        })
    }

    pub fn save_config(&self) -> Result<(), Box<dyn std::error::Error>> {
        let config_path = self.context.environment.data_dir.join("config.toml");

        self.context.config.save(&config_path)?;

        Ok(())
    }

    pub fn create_import_service(&self) -> ImportService<'_> {
        let import_dir = crate::paths::expand_path(&self.context.config.folders.csv_import);

        ImportService::new(
            &self.repository,
            import_dir,
            &self.context.environment.archive_dir,
        )
    }

    pub fn get_timesheets(
        &self,
    ) -> Result<Vec<crate::models::TimesheetEntry>, Box<dyn std::error::Error>> {
        Ok(self.repository.get_all()?)
    }

    pub fn get_current_pay_rate_for_personal_assistant(
        &self,
        personal_assistant_id: i64,
    ) -> Result<
        Option<crate::pay_rate_repository::PersonalAssistantPayRate>,
        Box<dyn std::error::Error>,
    > {
        Ok(self
            .pay_rate_repository
            .get_current_for_personal_assistant(personal_assistant_id)?)
    }

    pub fn get_payroll_schedule(
        &self,
        payroll_year: &str,
    ) -> Result<Vec<crate::payroll_schedule_repository::PayrollSchedule>, Box<dyn std::error::Error>>
    {
        Ok(self
            .payroll_schedule_repository
            .get_all_for_year(payroll_year)?)
    }

    pub fn import_csv(
        &self,
    ) -> Result<crate::import_service::ImportSummary, Box<dyn std::error::Error>> {
        Ok(self.create_import_service().run()?)
    }

    pub fn import_payroll_prep_sheet(
        &self,
        path: &std::path::Path,
    ) -> Result<usize, Box<dyn std::error::Error>> {
        let service = PayrollPrepSheetImportService::new(&self.payroll_schedule_repository);

        Ok(service.import(path)?)
    }

    pub fn import_payroll_return(
        &self,
        path: &std::path::Path,
        _payroll_year: &str,
        cycle_number: i64,
    ) -> Result<
        crate::archive::PayrollReturnImportResult,
        Box<dyn std::error::Error>,
    > {
        let assistants = self.personal_assistant_repository.get_all()?;

        let payslip_folder =
            crate::paths::expand_path(&self.context.config.folders.payslip_folder);

        let information_folder =
            crate::paths::expand_path(&self.context.config.folders.email_archive);

        Ok(crate::archive::import_payroll_return(
            path,
            &payslip_folder,
            &information_folder,
            &assistants,
            cycle_number,
        )?)
    }

    #[allow(dead_code)]
    pub fn generate_timesheet_pdf(
        &self,
        data: &TimesheetPdfData<'_>,
    ) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
        let output_dir = crate::paths::expand_path(&self.context.config.folders.pdf_output);

        Ok(PdfGenerator::generate(
            &output_dir,
            data,
            &self.context.config.pdf,
        )?)
    }

    pub fn send_payslip_email(
        &self,
        employer_email: &str,
        recipient_email: &str,
        personal_assistant_name: &str,
        payroll_week: i64,
        payslip_path: &std::path::Path,
        email_signature: Option<&str>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        crate::email_service::send_payslip_email(
            employer_email,
            recipient_email,
            personal_assistant_name,
            payroll_week,
            payslip_path,
            email_signature,
        )
    }

}

