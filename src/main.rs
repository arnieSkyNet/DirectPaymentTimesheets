mod app;
mod application;
mod archive;
mod config;
mod context;
mod csv_import;
mod database;
mod employer_repository;
mod employer_screen;
mod environment;
mod error;
mod gui;
mod import_service;
mod models;
mod paths;
mod pay_rate_repository;
mod personal_assistant_repository;
mod personal_assistant_screen;
mod repository;

fn main() {
    if let Err(error) = application::run() {
        eprintln!("Application error: {}", error);
    }
}
