mod annual_leave_settings_repository;
mod annual_leave_summary;
mod app;
mod application;
mod application_settings_screen;
mod archive;
mod backup_service;
mod config;
mod context;
mod contracted_hours_repository;
mod csv_import;
mod database;
mod direct_shift_repository;
mod email_service;
mod employer_repository;
mod employer_screen;
mod enter_hours_screen;
mod environment;
mod error;
mod folder_opener;
mod gui;
mod gui_controls;
mod historical_payroll_backfill;
mod import_service;
mod models;
mod paths;
mod pay_rate_allocation;
mod pay_rate_repository;
mod payroll_evidence;
mod payroll_file_naming;
mod payroll_prep_sheet_import_service;
mod payroll_provider_repository;
mod payroll_schedule_repository;
mod payroll_settings_screen;
mod payroll_snapshot_service;
mod payroll_timesheet_email_repository;
mod payroll_timesheet_repository;
mod payroll_timesheet_screen;
mod payroll_worked_item_repository;
mod payslip_delivery_service;
mod pdf_generator;
mod personal_assistant_repository;
mod personal_assistant_screen;
mod repository;
mod theme;
mod update_check;

fn main() {
    if let Err(error) = application::run() {
        eprintln!("Application error: {}", error);
    }
}
