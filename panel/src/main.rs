//! AIMAXXING Panel — native egui desktop control panel.
//!
//! Usage:
//!   aimaxxing-panel                        # connects to localhost:3000
//!   aimaxxing-panel --url http://host:3000  # connects to remote gateway
//!   aimaxxing-panel --url https://my-host.ts.net  # Tailscale

#![allow(dead_code)]
#![allow(unused_imports)]

mod api;
mod app;
mod app_state;
mod i18n;

use app::ClawPanel;
use eframe::egui::Vec2;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result<()> {
    // Init logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .compact()
        .init();

    // Parse optional --url arg
    let args: Vec<String> = std::env::args().collect();
    let url_override = args.windows(2).find_map(|w| {
        if w[0] == "--url" {
            Some(w[1].clone())
        } else {
            None
        }
    });

    // Tokio runtime for async HTTP calls
    let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
    let rt_handle = rt.handle().clone();

    // Keep runtime alive in a background thread
    std::thread::spawn(move || rt.block_on(std::future::pending::<()>()));

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("AIMAXXING // Control Panel")
            .with_inner_size(Vec2::new(1280.0, 780.0))
            .with_min_inner_size(Vec2::new(800.0, 500.0))
            .with_resizable(true)
            .with_icon(load_icon()),
        centered: true,
        // Use GPU hardware acceleration, never fall back to software rasterizer
        hardware_acceleration: eframe::HardwareAcceleration::Required,
        ..Default::default()
    };

    eframe::run_native(
        "AIMAXXING Panel",
        options,
        Box::new(move |cc| {
            let mut panel = ClawPanel::new(cc, rt_handle);

            // Apply --url override if provided
            if let Some(url) = url_override {
                panel.state_mut().set_url(url);
            }

            Ok(Box::new(panel))
        }),
    )
}

#[cfg(target_arch = "wasm32")]
fn main() {}

// ── WASM Entry Point ────────────────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub async fn start() -> std::result::Result<(), wasm_bindgen::JsValue> {
    let canvas_id = "the_canvas_id";

    // Init logging and panic hooks for WASM
    console_error_panic_hook::set_once();
    tracing_wasm::set_as_global_default();

    // In WASM, we don't have a multi-threaded tokio runtime.
    let web_options = eframe::WebOptions::default();

    let document = web_sys::window().unwrap().document().unwrap();
    let canvas = document.get_element_by_id(canvas_id).unwrap();
    let canvas = wasm_bindgen::JsCast::dyn_into::<web_sys::HtmlCanvasElement>(canvas).unwrap();

    eframe::WebRunner::new()
        .start(
            canvas,
            web_options,
            Box::new(move |cc| {
                let panel = ClawPanel::new(cc);
                Ok(Box::new(panel) as Box<dyn eframe::App>)
            }),
        )
        .await
}






/// Load a minimal embedded icon (a simple blue square as placeholder).
fn load_icon() -> eframe::egui::viewport::IconData {
    // 16x16 RGBA icon — blue dot on transparent background
    let size = 16usize;
    let mut pixels = vec![0u8; size * size * 4];
    for y in 2..14 {
        for x in 2..14 {
            let i = (y * size + x) * 4;
            pixels[i] = 59; // R
            pixels[i + 1] = 130; // G
            pixels[i + 2] = 246; // B
            pixels[i + 3] = 255; // A
        }
    }
    eframe::egui::viewport::IconData {
        rgba: pixels,
        width: size as u32,
        height: size as u32,
    }
}
