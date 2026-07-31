mod config;
mod models;
mod error;
mod database;


fn main() {
    println!("DirectPaymentTimesheets v0.0.1");
    println!("Application foundation ready.");

    database::initialise_database()
        .expect("Failed to initialise database");
}

