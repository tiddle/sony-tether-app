//! Pre-alpha tether app for the Sony A7 IV.
//!
//! Milestone M2 is where this becomes useful: live view, shutter release and
//! automatic download. Today it shows the window, the wiring between the UI and
//! the backend, and an honest status line when the Sony SDK is not configured.

mod review;
mod ui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1120.0, 720.0])
            .with_min_inner_size([760.0, 520.0])
            .with_title("Sony tether (pre-alpha)"),
        ..Default::default()
    };

    eframe::run_native(
        "sony-tether",
        options,
        Box::new(|_cc| Ok(Box::new(ui::TetherApp::new()))),
    )
}
