#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

// Declare modules needed by lib.rs or main itself
mod app_state;
mod commands;
mod config;
mod api;
mod monitoring;
mod models;
mod utils;
mod error;

fn main() {
    // Initialize the logger
    // Use environment variables (like RUST_LOG=info) to control level
    env_logger::init();

    // Call the run function from the library crate
    // The run function now contains the Tauri setup logic
    mc_hoster_backend::run(); // Assuming your crate name is mc_hoster_backend
}