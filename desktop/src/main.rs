// SPDX-License-Identifier: Apache-2.0

//! OpenWorld for Linux and Windows.
//!
//! `openworld-desktop [FILE]` opens the window, with FILE on the Review page.
//! `--self-check` builds the window, waits for startup, and quits (CI).
//! `--screenshot DIR` drives every screen and saves PNGs (built with `--features shots`).

#[cfg(feature = "shots")]
mod shots;
mod ui;

use std::path::PathBuf;

fn main() -> gtk::glib::ExitCode {
    let mut mode = ui::Mode::Normal;
    let mut files = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--self-check" => mode = ui::Mode::SelfCheck,
            "--screenshot" => match args.next() {
                Some(dir) => mode = ui::Mode::Shots(PathBuf::from(dir)),
                None => {
                    eprintln!("--screenshot needs a folder");
                    return gtk::glib::ExitCode::from(2);
                }
            },
            _ => files.push(arg),
        }
    }
    if let ui::Mode::Shots(_) = &mode {
        // Screenshots use a fresh data folder, never the person's own.
        let data = std::env::temp_dir().join(format!("openworld-shots-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        std::env::set_var("XDG_DATA_HOME", &data);
    }
    ui::run(mode, files)
}
