// Hide the console window on Windows in release builds (this is a GUI app).
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

fn main() -> eframe::Result<()> {
    // Log to stderr when RUST_LOG is set; silent otherwise.
    let _ = env_logger::try_init();
    catchyt::app::run()
}
