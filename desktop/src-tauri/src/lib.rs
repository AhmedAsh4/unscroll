pub mod adb;
pub mod app_state;
pub mod commands;
pub mod device;
pub mod recovery;
pub mod policy;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(app_state::UnscrollState::new())
        .invoke_handler(tauri::generate_handler![
            commands::handlers::discover_devices,
            commands::handlers::inspect_device,
            commands::handlers::get_session,
            commands::handlers::start_apply,
            commands::handlers::respond_to_decision,
            commands::handlers::start_edit,
            commands::handlers::open_maintenance,
            commands::handlers::close_maintenance,
            commands::handlers::start_restore,
            commands::handlers::retry_cleanup,
            commands::handlers::preview_diagnostics,
            commands::handlers::export_diagnostics,
            commands::handlers::load_app_icon
        ])
        .run(tauri::generate_context!())
        .expect("error while running Unscroll");
}
pub mod transaction;
