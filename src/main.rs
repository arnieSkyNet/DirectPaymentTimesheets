mod config;
mod models;
mod error;
mod database;
mod repository;
mod csv_import;
mod environment;
mod context;
mod paths;
mod archive;

use repository::TimesheetRepository;
use rusqlite::Connection;

fn main() {
    println!("DirectPaymentTimesheets v0.0.1");
    println!("Application foundation ready.");

    let context = context::AppContext::initialise()
        .expect("Failed to initialise application context");

    println!(
        "Application data directory: {:?}",
        context.environment.data_dir
    );

    database::initialise_database(&context.environment.database_path)
        .expect("Failed to initialise database");

    let connection = Connection::open(&context.environment.database_path)
        .expect("Failed to open database");

    let repository = TimesheetRepository::new(connection);

    let import_dir = paths::expand_path(&context.config.folders.csv_import);

    let csv_file = import_dir.join("sample_timesheets.csv");

    let csv_entries = match csv_import::import_csv(&csv_file) {
        Ok(entries) => entries,

        Err(error) => {
            let import_time = chrono::Local::now()
                .format("%Y-%m-%d %H:%M:%S")
                .to_string();

            repository.add_import_audit(
                &import_time,
                csv_file.to_string_lossy().as_ref(),
                "",
                0,
                0,
                0,
                "FAILED",
                Some(&error.to_string()),
            )
            .expect("Failed to write failed import audit");

            println!("Import failed: {}", error);

            return;
        }
    };

    let rows_processed = csv_entries.len() as i64;
    let mut rows_imported = 0i64;
    let mut rows_skipped = 0i64;

    for entry in csv_entries {
        if repository.exists(&entry)
            .expect("Failed to check existing timesheet")
        {
            println!(
                "Skipping existing entry: {} {}",
                entry.pa_name,
                entry.start_time
            );

            rows_skipped += 1;
        } else {
            repository.insert(&entry)
                .expect("Failed to insert timesheet");

            println!(
                "Imported: {} {}",
                entry.pa_name,
                entry.start_time
            );

            rows_imported += 1;
        }
    }

    let archive_file = archive::archive_csv(
        &csv_file,
        &context.environment.archive_dir,
    )
    .expect("Failed to archive CSV");

    println!("Archived CSV: {:?}", archive_file);

    let archive_filename = archive_file.to_string_lossy();

    let import_time = chrono::Local::now()
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();

    repository.add_import_audit(
        &import_time,
        csv_file.to_string_lossy().as_ref(),
        archive_filename.as_ref(),
        rows_processed,
        rows_imported,
        rows_skipped,
        "SUCCESS",
        None,
    )

    .expect("Failed to write import audit");

    let entries = repository.get_all()
        .expect("Failed to read timesheets");

    println!("Database contains {} entries.", entries.len());

    for entry in entries {
        println!("{:?}", entry);
    }
}

