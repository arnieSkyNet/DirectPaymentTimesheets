use rusqlite::Connection;

use crate::context::AppContext;
use crate::import_service::ImportService;
use crate::repository::TimesheetRepository;

pub struct Application {
    pub context: AppContext,
    pub repository: TimesheetRepository,
}

impl Application {
    pub fn initialise() -> Result<Self, Box<dyn std::error::Error>> {

        let context = AppContext::initialise()?;

        let connection = Connection::open(
            &context.environment.database_path
        )?;

        let repository = TimesheetRepository::new(connection);

        Ok(Self {
            context,
            repository,
        })
    }

    pub fn create_import_service(
        &self,
    ) -> ImportService<'_> {

        let import_dir =
            crate::paths::expand_path(
                &self.context.config.folders.csv_import
            );

        ImportService::new(
            &self.repository,
            import_dir,
            &self.context.environment.archive_dir,
        )
    }
}
