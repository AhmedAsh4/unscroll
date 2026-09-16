pub mod adb;
pub mod device;
pub mod recovery;
pub mod policy;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running Unscroll");
}
pub mod transaction;
