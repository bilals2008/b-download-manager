// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // A copy of the application that was started with the monitor flag does
    // not open a window: it watches the application from the outside and
    // writes down what happened when it ends badly.
    if let Some(folder) = my_idm_clone_lib::crash::monitor_folder() {
        my_idm_clone_lib::crash::watch(folder);
        return;
    }
    my_idm_clone_lib::run()
}
