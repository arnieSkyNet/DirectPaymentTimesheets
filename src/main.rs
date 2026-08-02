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
mod import_service;
mod application;

fn main() {
    if let Err(error) = application::run() {
        eprintln!("Application error: {}", error);
    }
}
