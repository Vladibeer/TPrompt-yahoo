// Скрива конзолния прозорец в release на Windows
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    tprompt_lib::run()
}
