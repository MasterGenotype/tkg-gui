mod app;
mod core;
mod data;
mod settings;
mod tabs;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("TKG Kernel Builder")
            .with_min_inner_size([900.0, 700.0]),
        ..Default::default()
    };

    eframe::run_native(
        "TKG Kernel Builder",
        options,
        Box::new(|_cc| Ok(Box::new(app::TkgApp::new()))),
    )
}
