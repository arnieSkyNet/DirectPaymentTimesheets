use rusqlite::Connection;

use crate::context::AppContext;
use crate::employer_repository::EmployerRepository;
use crate::import_service::ImportService;
use crate::pay_rate_repository::PayRateRepository;
use crate::personal_assistant_repository::PersonalAssistantRepository;
use crate::repository::TimesheetRepository;

pub struct Application {
    pub context: AppContext,
    pub repository: TimesheetRepository,
    pub employer_repository: EmployerRepository,
    pub personal_assistant_repository: PersonalAssistantRepository,
    pub pay_rate_repository: PayRateRepository,
}

impl Application {
    pub fn initialise() -> Result<Self, Box<dyn std::error::Error>> {
        let context = AppContext::initialise()?;

        let timesheet_connection = Connection::open(&context.environment.database_path)?;

        let employer_connection = Connection::open(&context.environment.database_path)?;

        let personal_assistant_connection = Connection::open(&context.environment.database_path)?;

        let pay_rate_connection = Connection::open(&context.environment.database_path)?;

        let repository = TimesheetRepository::new(timesheet_connection);

        let employer_repository = EmployerRepository::new(employer_connection);

        let personal_assistant_repository =
            PersonalAssistantRepository::new(personal_assistant_connection);

        let pay_rate_repository = PayRateRepository::new(pay_rate_connection);

        Ok(Self {
            context,
            repository,
            employer_repository,
            personal_assistant_repository,
            pay_rate_repository,
        })
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
}
