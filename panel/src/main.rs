//! AIMAXXING Panel — native egui desktop control panel.
use eframe::egui::Vec2;

mod api;
mod app;
mod app_state;
mod i18n;

use app::ClawPanel;

fn main() -> eframe::Result<()> {
    // 强制启动一个隔离的 Tokio 运行时
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to create Tokio runtime");

    let handle = rt.handle().clone();

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("AIMAXXING // Control Panel")
            .with_inner_size(Vec2::new(1024.0, 768.0))
            .with_resizable(true),
        ..Default::default()
    };

    eframe::run_native(
        "aimaxxing-panel",
        options,
        Box::new(move |cc| {
            // 直接传入预先准备好的 handle，不再依赖 Handle::current()
            Ok(Box::new(ClawPanel::new(cc, handle)))
        }),
    )
}
