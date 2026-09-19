// Suppress the console window that Windows would otherwise open alongside a
// release build.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    cl_lib::run()
}
