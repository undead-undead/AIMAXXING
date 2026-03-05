//! Minimal egui app to test resize performance.
//! Run with: cargo run -p aimaxxing-panel --release --example resize_test

use eframe::egui;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Resize Test — Minimal")
            .with_inner_size(egui::vec2(800.0, 500.0))
            .with_resizable(true)
            .with_decorations(true),
        hardware_acceleration: eframe::HardwareAcceleration::Required,
        vsync: false,
        ..Default::default()
    };

    eframe::run_native(
        "resize_test",
        options,
        Box::new(|_cc| Ok(Box::new(MinimalApp))),
    )
}

struct MinimalApp;

impl eframe::App for MinimalApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let sr = ctx.screen_rect();
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Resize Test");
            ui.label(format!(
                "Window: {:.0} x {:.0} | ppp: {:.2}",
                sr.width(),
                sr.height(),
                ctx.pixels_per_point()
            ));
            ui.label("Try resizing this window. Is it smooth or laggy?");
        });
    }
}
