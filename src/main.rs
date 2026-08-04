mod app;
mod application;
mod archive;
mod config;
mod context;
mod csv_import;
mod database;
mod environment;
mod error;
mod gui;
mod import_service;
mod models;
mod paths;
mod repository;

fn main() {
    if let Err(error) = application::run() {
        eprintln!("Application error: {}", error);
    }
}
