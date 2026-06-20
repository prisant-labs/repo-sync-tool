// RepoSync desktop binary entry point.
//
// Owning effort: E-01 (Foundation).
// Prevents an extra console window on Windows in release builds. Do not remove.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    reposync_lib::run()
}
