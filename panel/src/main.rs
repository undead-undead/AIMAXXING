//! AIMAXXING Panel — native egui desktop control panel.
use eframe::egui::Vec2;

mod api;
mod app;
mod app_state;
mod i18n;

use app::ClawPanel;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;


#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(target_arch = "wasm32")]
fn main() {
    // Redirect `log` messages to `console.log` and friends:
    eframe::WebLogger::init(log::LevelFilter::Debug).ok();

    let web_options = eframe::WebOptions::default();

    wasm_bindgen_futures::spawn_local(async {
        let canvas = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("the_canvas_id"))
            .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok())
            .expect("Failed to find canvas element");

        eframe::WebRunner::new()
            .start(
                canvas,
                web_options,
                Box::new(|cc| Ok(Box::new(ClawPanel::new(cc)))),
            )
            .await
            .expect("failed to start eframe");
    });
}


