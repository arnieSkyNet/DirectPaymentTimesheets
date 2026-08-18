use rusqlite::Connection;

use crate::context::AppContext;
use crate::contracted_hours_repository::ContractedHoursRepository;
use crate::employer_repository::EmployerRepository;
use crate::import_service::ImportService;
use crate::pay_rate_repository::PayRateRepository;
use crate::payroll_prep_sheet_import_service::PayrollPrepSheetImportService;
use crate::payroll_provider_repository::PayrollProviderRepository;
use crate::payroll_schedule_repository::PayrollScheduleRepository;
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
}

impl Application {
    pub fn initialise() -> Result<Self, Box<dyn std::error::Error>> {
        let context = AppContext::initialise()?;

        let timesheet_connection = Connection::open(&context.environment.database_path)?;

        let employer_connection = Connection::open(&context.environment.database_path)?;

        let personal_assistant_connection = Connection::open(&context.environment.database_path)?;

        let pay_rate_connection = Connection::open(&context.environment.database_path)?;
        let contracted_hours_connection = Connection::open(&context.environment.database_path)?;

        let repository = TimesheetRepository::new(timesheet_connection);

        let employer_repository = EmployerRepository::new(employer_connection);

        let personal_assistant_repository =
            PersonalAssistantRepository::new(personal_assistant_connection);

        let pay_rate_repository = PayRateRepository::new(pay_rate_connection);

        let contracted_hours_repository =
            ContractedHoursRepository::new(contracted_hours_connection);

        let payroll_provider_connection = Connection::open(&context.environment.database_path)?;

        let payroll_provider_repository =
            PayrollProviderRepository::new(payroll_provider_connection);

        let payroll_schedule_connection = Connection::open(&context.environment.database_path)?;

        let payroll_schedule_repository =
            PayrollScheduleRepository::new(payroll_schedule_connection);

        Ok(Self {
            context,
            repository,
            employer_repository,
            personal_assistant_repository,
            pay_rate_repository,
            contracted_hours_repository,
            payroll_provider_repository,
            payroll_schedule_repository,
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
        let timesheets = self.repository.get_all()?;

        Ok(timesheets)
    }

    pub fn import_csv(
        &self,
    ) -> Result<crate::import_service::ImportSummary, Box<dyn std::error::Error>> {
        let service = self.create_import_service();

        let summary = service.run()?;

        Ok(summary)
    }

    pub fn import_payroll_prep_sheet(
        &self,
        path: &std::path::Path,
    ) -> Result<usize, Box<dyn std::error::Error>> {
        let service = PayrollPrepSheetImportService::new(&self.payroll_schedule_repository);

        service.import(path)
    }

    pub fn generate_timesheet_pdf(
        &self,
        data: &TimesheetPdfData<'_>,
    ) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
        let output_dir = crate::paths::expand_path(&self.context.config.folders.pdf_output);

        let output_path = PdfGenerator::generate(&output_dir, data)?;

        Ok(output_path)
    }
}
