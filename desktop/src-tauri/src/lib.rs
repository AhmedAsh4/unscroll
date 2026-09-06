pub mod adb;
pub mod device;
pub mod recovery {
    pub mod model;
    pub mod shared_copy;
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running Unscroll");
}
