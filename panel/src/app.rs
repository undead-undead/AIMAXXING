use crate::api::{BlueprintInfo, MarketSkill, SkillInfo};
use crate::app_state::{ActiveTab, AppState, PersonaSubTab, SkillsSubTab, VaultEntry, ApiSubTab};
use crate::i18n::{t, Language};
use eframe::egui::{self, Color32, FontId, RichText, Stroke};

use poll_promise::Promise;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::spawn_local;

/// Colour palette — dynamic based on theme.
mod palette {
    use eframe::egui::Color32;
    
    pub fn bg_deep(night: bool) -> Color32 {
        if night { Color32::from_rgb(9, 9, 11) } else { Color32::from_rgb(240, 240, 245) }
    }
    
    pub fn bg_surface(night: bool) -> Color32 {
        if night { Color32::from_rgb(24, 24, 28) } else { Color32::from_rgb(255, 255, 255) }
    }

    pub const ACCENT: Color32 = Color32::from_rgb(102, 178, 255);
    pub const DANGER: Color32 = Color32::from_rgb(239, 68, 68);
    pub const SUCCESS: Color32 = Color32::from_rgb(34, 197, 94);
    pub const WARNING: Color32 = Color32::from_rgb(234, 179, 8);
    pub const INFO: Color32 = Color32::from_rgb(14, 165, 233);
    
    pub fn text_dim(night: bool) -> Color32 {
        if night { Color32::from_rgb(160, 160, 170) } else { Color32::from_rgb(100, 100, 110) }
    }
    
    pub fn text_bright(night: bool) -> Color32 {
        if night { Color32::from_rgb(240, 240, 250) } else { Color32::from_rgb(20, 20, 30) }
    }
    
    pub fn border(night: bool) -> Color32 {
        if night { Color32::from_rgb(60, 60, 70) } else { Color32::from_rgb(200, 200, 210) }
    }
    
    pub const TAG_BG: Color32 = Color32::from_rgba_premultiplied(102, 178, 255, 30);
}

// ── ClawPanel struct ─────────────────────────────────────────────────────────

pub struct ClawPanel {
    state: AppState,
    #[cfg(not(target_arch = "wasm32"))]
    rt: tokio::runtime::Handle,
    #[cfg(not(target_arch = "wasm32"))]
    tray_icon: Option<tray_icon::TrayIcon>,
}

impl ClawPanel {
    fn do_full_shutdown(&mut self, _ctx: &egui::Context) {
        let client = self.state.client.clone();
        #[cfg(not(target_arch = "wasm32"))]
        {
            spawn_task(&self.rt, async move {
                let _ = client.shutdown_gateway().await;
                // The gateway will exit in 1s. We close ourselves now.
                std::process::exit(0);
            });
        }
        #[cfg(target_arch = "wasm32")]
        {
            spawn_task(async move {
                let _ = client.shutdown_gateway().await;
            });
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn new(cc: &eframe::CreationContext<'_>, rt: tokio::runtime::Handle) -> Self {
        Self::init(cc, rt)
    }

    #[cfg(target_arch = "wasm32")]
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        Self::init(cc)
    }

    fn init(
        cc: &eframe::CreationContext<'_>,
        #[cfg(not(target_arch = "wasm32"))] rt: tokio::runtime::Handle,
    ) -> Self {
        let mut visuals = if cc.egui_ctx.style().visuals.dark_mode {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        
        // Initial theme application
        let night = true; // AppState::new() defaults to night=true
        visuals.panel_fill = palette::bg_deep(night);
        visuals.window_fill = palette::bg_surface(night);
        visuals.widgets.noninteractive.bg_fill = palette::bg_surface(night);
        visuals.widgets.inactive.bg_fill = palette::bg_surface(night);
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, palette::border(night));
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, palette::ACCENT);
        visuals.selection.bg_fill = Color32::from_rgba_premultiplied(102, 178, 255, 60);
        cc.egui_ctx.set_visuals(visuals);

        let mut style = (*cc.egui_ctx.style()).clone();
        style.text_styles.insert(
            egui::TextStyle::Body,
            FontId::new(14.0, egui::FontFamily::Proportional),
        );
        style.text_styles.insert(
            egui::TextStyle::Button,
            FontId::new(13.0, egui::FontFamily::Proportional),
        );
        cc.egui_ctx.set_style(style);

        // Sub-phase 4: Font setup (P11)
        Self::setup_fonts(&cc.egui_ctx);

        let mut panel = Self {
            state: AppState::new(),
            #[cfg(not(target_arch = "wasm32"))]
            rt,
            #[cfg(not(target_arch = "wasm32"))]
            tray_icon: None,
        };

        #[cfg(not(target_arch = "wasm32"))]
        panel.init_tray();
        panel.trigger_refresh(&cc.egui_ctx);

        panel
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn init_tray(&mut self) {
        use tray_icon::{Icon, TrayIconBuilder};

        // Create a solid blue 16x16 icon
        let size = 16;
        let mut pixels = vec![0u8; size * size * 4];
        for i in 0..size * size {
            pixels[i * 4] = 59;     // R
            pixels[i * 4 + 1] = 130; // G
            pixels[i * 4 + 2] = 246; // B
            pixels[i * 4 + 3] = 255; // A
        }

        if let Ok(icon) = Icon::from_rgba(pixels, size as u32, size as u32) {
            match TrayIconBuilder::new()
                .with_tooltip("AIMAXXING Control Panel")
                .with_icon(icon)
                .build()
            {
                Ok(tray) => {
                    self.tray_icon = Some(tray);
                }
                Err(e) => {
                    tracing::warn!("Failed to build tray icon: {}", e);
                }
            }
        }
    }

    pub fn state_mut(&mut self) -> &mut AppState {
        &mut self.state
    }

    fn setup_fonts(ctx: &egui::Context) {
        let mut fonts = egui::FontDefinitions::default();
        
        // 1. 注入内置字体 (作为保底)
        fonts.font_data.insert(
            "noto_sans_sc".to_owned(),
            egui::FontData::from_static(include_bytes!("../assets/NotoSansSC-Regular.subset.ttf")).into(),
        );
        // 2. 尝试从用户系统加载商业级字体 (增强显示效果)
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut sys_font_paths = Vec::new();
            
            #[cfg(target_os = "windows")]
            {
                sys_font_paths.push("C:\\Windows\\Fonts\\msyh.ttc");
                sys_font_paths.push("C:\\Windows\\Fonts\\msyh.ttf");
            }
            
            #[cfg(target_os = "macos")]
            {
                sys_font_paths.push("/System/Library/Fonts/PingFang.ttc");
            }
            
            #[cfg(target_os = "linux")]
            {
                sys_font_paths.push("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc");
                sys_font_paths.push("/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc");
                sys_font_paths.push("/usr/share/fonts/truetype/wqy/wqy-microhei.ttc");
            }
            for path in sys_font_paths {
                if std::path::Path::new(path).exists() {
                    if let Ok(data) = std::fs::read(path) {
                        fonts.font_data.insert(
                            "system_fallback".to_owned(),
                            egui::FontData::from_owned(data).into(),
                        );
                        break;
                    }
                }
            }
        }
        // 3. 设置优先级：优先内置字体 → 系统后备 → egui 默认
        let families = [egui::FontFamily::Proportional, egui::FontFamily::Monospace];
        for family in families {
            let list = fonts.families.get_mut(&family).unwrap();
            list.insert(0, "noto_sans_sc".to_owned());
            if fonts.font_data.contains_key("system_fallback") {
                list.insert(1, "system_fallback".to_owned());
            }
        }
        
        ctx.set_fonts(fonts);
    }

    fn theme_bg_deep(&self) -> Color32 {
        palette::bg_deep(self.state.night_mode)
    }

    fn theme_bg_surface(&self) -> Color32 {
        palette::bg_surface(self.state.night_mode)
    }
}

// ── Platform-agnostic task spawner ───────────────────────────────────────────

#[cfg(not(target_arch = "wasm32"))]
fn spawn_task(
    rt: &tokio::runtime::Handle,
    future: impl std::future::Future<Output = ()> + Send + 'static,
) {
    rt.spawn(future);
}

#[cfg(target_arch = "wasm32")]
fn spawn_task(future: impl std::future::Future<Output = ()> + 'static) {
    spawn_local(future);
}

// ── eframe::App impl ─────────────────────────────────────────────────────────

impl eframe::App for ClawPanel {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Theme persistence and real-time syncing
        let is_dark_in_ctx = ctx.style().visuals.dark_mode;
        if self.state.night_mode != is_dark_in_ctx {
            let night = self.state.night_mode;
            let mut visuals = if night { egui::Visuals::dark() } else { egui::Visuals::light() };
            
            visuals.panel_fill = palette::bg_deep(night);
            visuals.window_fill = palette::bg_surface(night);
            visuals.widgets.noninteractive.bg_fill = palette::bg_surface(night);
            visuals.widgets.inactive.bg_fill = if night { Color32::from_rgb(14, 14, 18) } else { Color32::from_rgb(245, 245, 250) };
            visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, palette::border(night));
            visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, palette::ACCENT);
            visuals.selection.bg_fill = Color32::from_rgba_premultiplied(102, 178, 255, 60);
            
            ctx.set_visuals(visuals);
            crate::app_state::save_config(&self.state);
        }

        // Handle Tray Icon Events
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Ok(event) = tray_icon::TrayIconEvent::receiver().try_recv() {
                use tray_icon::TrayIconEvent;
                match event {
                    TrayIconEvent::Click { .. } | TrayIconEvent::DoubleClick { .. } => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    }
                    _ => {}
                }
            }
        }
        
        // Top bar
        egui::TopBottomPanel::top("top_bar")
            .frame(
                egui::Frame::new()
                    .fill(self.theme_bg_deep())
                    .inner_margin(egui::Margin::symmetric(12, 8)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("◈ AIMAXXING")
                            .color(palette::ACCENT)
                            .font(FontId::new(16.0, egui::FontFamily::Monospace))
                            .strong(),
                    );
                    ui.separator();
                    let (dot, dot_color) = match self.state.connected {
                        Some(true) => ("●", palette::SUCCESS),
                        Some(false) => ("●", palette::DANGER),
                        None => ("○", palette::text_dim(self.state.night_mode)),
                    };
                    ui.label(RichText::new(dot).color(dot_color).small());
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(if self.state.connected == Some(true) {
                            t("misc.connected", self.state.language)
                        } else {
                            t("misc.disconnected", self.state.language)
                        })
                        .small()
                        .color(palette::text_dim(self.state.night_mode)),
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Emergency Brake (Phase 12)
                        // This sends a global cancel signal to all agents
                        if self.state.cancel_promise.is_some() {
                             ui.spinner();
                        } else {
                            let stop_btn = egui::Button::new(RichText::new("🛑").strong())
                                .fill(palette::DANGER.gamma_multiply(0.2))
                                .stroke(egui::Stroke::new(1.0, palette::DANGER));
                            
                            if ui.add(stop_btn)
                                .on_hover_text(t("system.emergency_brake", self.state.language))
                                .clicked() 
                            {
                                let client = self.state.client.clone();
                                let (sender, promise) = Promise::new();
                                self.state.cancel_promise = Some(promise);
                                #[cfg(not(target_arch = "wasm32"))]
                                spawn_task(&self.rt, async move {
                                    sender.send(client.cancel_task().await.map_err(|e| e.to_string()));
                                });
                                #[cfg(target_arch = "wasm32")]
                                spawn_task(async move {
                                    sender.send(client.cancel_task().await.map_err(|e| e.to_string()));
                                });
                            }
                        }

                        ui.separator();

                        // Language Switcher
                        let lang_text = if self.state.language == Language::Zh { "EN" } else { "中" };
                        if ui.button(RichText::new(lang_text).strong()).clicked() {
                            self.state.language = if self.state.language == Language::Zh { Language::En } else { Language::Zh };
                            crate::app_state::save_config(&self.state);
                        }
                        
                        ui.separator();

                        // Theme Toggle
                        let theme_icon = if self.state.night_mode { "☀" } else { "🌙" };
                        if ui.button(theme_icon).clicked() {
                            self.state.night_mode = !self.state.night_mode;
                            crate::app_state::save_config(&self.state);
                        }
                    });
                });
            });

        // Tab bar — full-width adaptive
        egui::TopBottomPanel::top("tabs")
            .frame(
                egui::Frame::new()
                    .fill(self.theme_bg_deep())
                    .inner_margin(egui::Margin::symmetric(0, 0)),
            )
            .show(ctx, |ui| {
                let avail_width = ui.available_width();
                let tabs = [
                    ("tabs.skills", ActiveTab::Skills),
                    ("tabs.vault", ActiveTab::Api),
                    ("tabs.dashboard", ActiveTab::Dashboard),
                    ("tabs.logs", ActiveTab::Logs),
                    ("tabs.sessions", ActiveTab::Sessions),
                    ("tabs.cron", ActiveTab::Cron),
                    ("tabs.persona", ActiveTab::Persona),
                    ("tabs.chat", ActiveTab::Chat),
                    ("tabs.system", ActiveTab::System),
                    ("tabs.connection", ActiveTab::Connection),
                ];
                let n_tabs = tabs.len() as f32;
                let btn_width = avail_width / n_tabs;
                let btn_height = 40.0;
                // Adaptive font size: scales between 11.0 and 16.0 based on window width
                let font_size = (avail_width / 60.0).clamp(11.0, 16.0);

                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    for (key, tab) in tabs {
                        let label = t(key, self.state.language);
                        let is_active = self.state.tab == tab;
                        let text = RichText::new(label)
                            .font(FontId::new(font_size, egui::FontFamily::Proportional))
                            .color(if is_active {
                                palette::text_bright(self.state.night_mode)
                            } else {
                                palette::text_dim(self.state.night_mode)
                            });
                        let response = ui.add_sized(
                            [btn_width, btn_height],
                            egui::Button::new(text)
                                .fill(if is_active {
                                    palette::bg_deep(self.state.night_mode)
                                } else {
                                    Color32::TRANSPARENT
                                })
                                .stroke(Stroke::NONE)
                                .corner_radius(0.0),
                        );
                        if response.clicked() {
                            self.state.tab = tab.clone();
                            crate::app_state::save_config(&self.state);
                            if self.state.tab == ActiveTab::Persona {
                                self.do_snapshot_refresh(ctx);
                            }
                        }
                    }
                });
            });

        // Status bar
        egui::TopBottomPanel::bottom("status_bar")
            .exact_height(24.0)
            .frame(
                egui::Frame::new()
                    .fill(self.theme_bg_deep())
                    .inner_margin(egui::Margin::symmetric(12, 4)),
            )
            .show(ctx, |ui| {
                if let Some((msg, is_error)) = &self.state.status_msg {
                    let color = if *is_error {
                        palette::DANGER
                    } else {
                        palette::text_dim(self.state.night_mode)
                    };
                    ui.label(RichText::new(msg).small().color(color));
                }
            });

        // Main content
        egui::CentralPanel::default()
            .frame(
                egui::Frame::NONE
                    .fill(self.theme_bg_surface())
                    .inner_margin(egui::Margin::symmetric(0, 14)), // Flush horizontal
            )
            .show(ctx, |ui| {
                // Logs and Chat manage their own internal ScrollAreas.
                // Wrapping them in an outer ScrollArea causes double-layout negotiation
                // which produces severe lag when resizing the window.
                match self.state.tab {
                    ActiveTab::Logs => self.show_logs_tab(ui, ctx),
                    ActiveTab::Chat => self.show_chat_tab(ui, ctx),
                    _ => {
                        egui::ScrollArea::vertical()
                            .auto_shrink([false; 2])
                            .show(ui, |ui| {
                                match self.state.tab.clone() {
                                    ActiveTab::Skills     => self.show_skills_tab(ui, ctx),
                                    ActiveTab::Api        => self.show_api_tab(ui, ctx),
                                    ActiveTab::Store      => {
                                         self.state.tab = ActiveTab::Skills;
                                         self.state.skills_subtab = SkillsSubTab::Market;
                                         self.show_skills_tab(ui, ctx);
                                     }
                                    ActiveTab::Sessions   => self.show_sessions_tab(ui, ctx),
                                    ActiveTab::Cron       => self.show_cron_tab(ui, ctx),
                                    ActiveTab::Persona    => self.show_persona_tab(ui, ctx),
                                    ActiveTab::Connection => self.show_connection_tab(ui, ctx),
                                    ActiveTab::Dashboard  => self.show_dashboard_tab(ui, ctx),
                                    ActiveTab::System     => self.show_system_tab(ui, ctx),
                                    ActiveTab::Channels   => self.show_channels_tab(ui, ctx),
                                    // Already handled above:
                                    ActiveTab::Logs | ActiveTab::Chat => {}
                                }
                            });
                    }
                }
            });

        self.poll_channel_promise();
        self.poll_sandbox_promises(ctx);
        self.poll_cancel_promise(ctx);
        self.poll_blueprint_promise(ctx);
        self.poll_skills_promise();
        self.poll_market_search_promise();
        self.poll_install_promise(ctx);
        self.poll_market_install_promise(ctx);
        self.poll_persona_export_promise(ctx);
        self.poll_provider_promise();


        // ── 定时任务 ────────────────────────────────────────────────────────
        // egui 的 i.time 是从程序启动到现在的秒数，跨平台（含 WASM）
        let now = ctx.input(|i| i.time);

        // ① 技能列表：每 30 秒自动刷新一次
        const SKILL_REFRESH_INTERVAL: f64 = 30.0;
        if now - self.state.last_skill_refresh_time > SKILL_REFRESH_INTERVAL
            && self.state.skills_promise.is_none()
        {
            self.state.last_skill_refresh_time = now;
            self.trigger_refresh(ctx);
        }

        // ② 日志：当停留在 Logs tab 且开启了 auto_log_poll，每 2 秒拉一次
        const LOG_POLL_INTERVAL: f64 = 2.0;
        if self.state.auto_log_poll
            && self.state.tab == ActiveTab::Logs
        {
            if now - self.state.last_log_poll_time > LOG_POLL_INTERVAL {
                self.state.last_log_poll_time = now;
                self.do_log_poll(ctx);
            }
        }

        // ③ 沙箱管理：每 10 秒刷新一次，或者手动刷新
        const SANDBOX_REFRESH_INTERVAL: f64 = 10.0;
        if now - self.state.last_sandboxes_refresh_time > SANDBOX_REFRESH_INTERVAL
            && self.state.sandboxes_promise.is_none()
        {
            self.state.last_sandboxes_refresh_time = now;
            self.do_sandboxes_refresh(ctx);
        }

        // ③ Sessions：在 Sessions tab 时每 30 秒刷一次
        const SESSIONS_INTERVAL: f64 = 30.0;
        if self.state.tab == ActiveTab::Sessions
            && now - self.state.last_sessions_refresh_time > SESSIONS_INTERVAL
        {
            self.state.last_sessions_refresh_time = now;
            self.do_sessions_refresh(ctx);
        }

        // ④ Cron jobs：在 Cron tab 时每 60 秒刷一次
        const CRON_INTERVAL: f64 = 60.0;
        if self.state.tab == ActiveTab::Cron
            && now - self.state.last_cron_refresh_time > CRON_INTERVAL
        {
            self.state.last_cron_refresh_time = now;
            self.do_cron_refresh(ctx);
        }

        // ⑤ Snapshot：每 60 秒自动刷一次（所有 tab 都刷，保存/删除不再触发快照）
        const SNAPSHOT_INTERVAL: f64 = 60.0;
        if now - self.state.last_snapshot_refresh_time > SNAPSHOT_INTERVAL {
            self.state.last_snapshot_refresh_time = now;
            self.do_snapshot_refresh(ctx);
        }

        // ⑥ Metrics：每 10 秒刷一次
        const METRICS_INTERVAL: f64 = 10.0;
        if now - self.state.last_metrics_refresh_time > METRICS_INTERVAL {
            self.state.last_metrics_refresh_time = now;
            self.do_metrics_refresh(ctx);
        }

        // 告诉 egui 下一次唤醒时间（避免 CPU 忙等）
        let next_skill = SKILL_REFRESH_INTERVAL
            - (now - self.state.last_skill_refresh_time).min(SKILL_REFRESH_INTERVAL);
        let next_log = if self.state.auto_log_poll && self.state.tab == ActiveTab::Logs {
            LOG_POLL_INTERVAL - (now - self.state.last_log_poll_time).min(LOG_POLL_INTERVAL)
        } else {
            SKILL_REFRESH_INTERVAL
        };
        let next_wake = next_skill.min(next_log).max(1.0);
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(next_wake));

        // Skill detail popup
        if self.state.expanded_skill.is_some() {
            self.show_skill_detail_window(ctx);
        }

        if self.state.persona_export_json.is_some() {
            self.show_export_result_window(ctx);
        }


        // ── 🌟 Smart Exit Guard ───────────────────────────────────────────
        if ctx.input(|i| i.viewport().close_requested()) {
            if !self.state.show_exit_dialog {
                self.state.show_exit_dialog = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
        }

        if self.state.show_exit_dialog {
            egui::Window::new("Exit AIMAXXING")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("Choose exit strategy:").strong());
                        ui.add_space(12.0);

                        ui.horizontal(|ui| {
                            if ui.button("  Option A: Full Shutdown  ").clicked() {
                                self.state.exit_in_progress = true;
                                self.do_full_shutdown(ctx);
                            }
                            if ui.button("  Option B: Minimize to Tray  ").clicked() {
                                // Minimize app, let it run in background
                                self.state.show_exit_dialog = false;
                                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                            }
                        });

                        if self.state.exit_in_progress {
                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label("Shutting down gateway...");
                            });
                        }

                        ui.add_space(8.0);
                        if ui.link("Cancel").clicked() {
                            self.state.show_exit_dialog = false;
                        }
                    });
                });
        }
    }
}

// ── Internal helpers ──────────────────────────────────────────────────────────

impl ClawPanel {
    fn trigger_refresh(&mut self, ctx: &egui::Context) {
        let client = self.state.client.clone();
        let ctx_clone = ctx.clone();
        
        // Update the timestamp to prevent the background poll from firing immediately after
        self.state.last_skill_refresh_time = ctx.input(|i| i.time);

        let (sender, promise) = Promise::new();

        #[cfg(not(target_arch = "wasm32"))]
        spawn_task(&self.rt, async move {
            let result = match client.list_skills().await {
                Ok(skills) => {
                    ctx_clone.request_repaint();
                    Ok(skills)
                }
                Err(e) => Err(e.to_string()),
            };
            sender.send(result);
        });
        #[cfg(target_arch = "wasm32")]
        spawn_task(async move {
            let result = match client.list_skills().await {
                Ok(skills) => {
                    ctx_clone.request_repaint();
                    Ok(skills)
                }
                Err(e) => Err(e.to_string()),
            };
            sender.send(result);
        });

        self.state.skills_promise = Some(promise);
        
        self.do_persona_refresh(ctx);
        self.do_snapshot_refresh(ctx);
        self.do_cron_refresh(ctx);
        self.do_sessions_refresh(ctx);
        self.do_channel_refresh(ctx);
        self.do_sandboxes_refresh(ctx);
        self.do_provider_refresh(ctx);
        self.do_persona_templates_refresh(ctx);

        // Health check
        let client2 = self.state.client.clone();
        #[cfg(not(target_arch = "wasm32"))]
        spawn_task(&self.rt, async move {
            let _ = client2.health().await;
        });
        #[cfg(target_arch = "wasm32")]
        spawn_task(async move {
            let _ = client2.health().await;
        });

        self.state.connected = None;
    }

    fn poll_skills_promise(&mut self) {
        let resolved = if let Some(ref promise) = self.state.skills_promise {
            match promise.ready() {
                Some(Ok(skills)) => Some(Ok(skills.clone())),
                Some(Err(e)) => Some(Err(e.clone())),
                None => None,
            }
        } else {
            None
        };

        if let Some(result) = resolved {
            self.state.skills_promise = None;
            match result {
                Ok(skills) => {
                    self.state.skills = skills;
                    self.state.connected = Some(true);
                    self.state.set_status(
                        format!("Loaded {} skills", self.state.skills.len()),
                        false,
                    );
                }
                Err(e) => {
                    self.state.connected = Some(false);
                    self.state.set_status(format!("Error: {}", e), true);
                }
            }
        }
    }

    fn do_market_search(&mut self, query: String, page: u32, ctx: &egui::Context) {
        let client = self.state.client.clone();
        let ctx_clone = ctx.clone();
        let (sender, promise) = Promise::new();
        self.state.market_loading = true;
        self.state.market_error = None;
        self.state.market_page = page;

        #[cfg(not(target_arch = "wasm32"))]
        spawn_task(&self.rt, async move {
            let result = match client.search_market(&query, page).await {
                Ok(skills) => {
                    ctx_clone.request_repaint();
                    Ok(skills)
                }
                Err(e) => Err(e.to_string()),
            };
            sender.send(result);
        });
        #[cfg(target_arch = "wasm32")]
        spawn_task(async move {
            let result = match client.search_market(&query, page).await {
                Ok(skills) => {
                    ctx_clone.request_repaint();
                    Ok(skills)
                }
                Err(e) => Err(e.to_string()),
            };
            sender.send(result);
        });

        self.state.market_search_promise = Some(promise);
    }

    fn poll_market_search_promise(&mut self) {
        if let Some(result) = self.state.market_search_promise.as_ref().and_then(|p| p.ready()) {
            match result {
                Ok(skills) => {
                    if self.state.market_page > 1 {
                        self.state.market_skills.extend(skills.clone());
                    } else {
                        self.state.market_skills = skills.clone();
                    }
                    self.state.market_loading = false;
                    self.state.market_error = None;
                }
                Err(e) => {
                    self.state.market_loading = false;
                    self.state.market_error = Some(e.clone());
                }
            }
            self.state.market_search_promise = None;
        }
    }

    fn do_market_install(&mut self, url: String, ctx: &egui::Context) {
        let client = self.state.client.clone();
        let ctx_clone = ctx.clone();
        let (sender, promise) = Promise::new();
        self.state.market_installing_url = Some(url.clone());
        self.state.market_install_error = None;
        self.state.market_install_success = None;
        self.state.set_status(format!("Installing skill..."), false);

        #[cfg(not(target_arch = "wasm32"))]
        spawn_task(&self.rt, async move {
            let result = match client.install_skill(&url).await {
                Ok(res) => {
                    let _ = client.toggle_skill(&res.skill_name).await;
                    ctx_clone.request_repaint();
                    Ok(res)
                }
                Err(e) => Err(e.to_string()),
            };
            sender.send(result);
        });
        #[cfg(target_arch = "wasm32")]
        spawn_task(async move {
            let result = match client.install_skill(&url).await {
                Ok(res) => {
                    let _ = client.toggle_skill(&res.skill_name).await;
                    ctx_clone.request_repaint();
                    Ok(res)
                }
                Err(e) => Err(e.to_string()),
            };
            sender.send(result);
        });

        self.state.market_install_promise = Some(promise);
    }

    fn poll_market_install_promise(&mut self, ctx: &egui::Context) {
        if let Some(result) = self.state.market_install_promise.as_ref().and_then(|p| p.ready()) {
            self.state.market_installing_url = None;
            match result {
                Ok(res) => {
                    self.state.market_install_success = Some(format!("Successfully installed: {}", res.skill_name));
                    self.state.set_status(format!("Successfully installed {}", res.skill_name), false);
                    self.trigger_refresh(ctx);
                }
                Err(e) => {
                    self.state.market_install_error = Some(e.clone());
                    self.state.set_status(format!("Install failed: {}", e), true);
                }
            }
            self.state.market_install_promise = None;
        }
    }

    fn poll_log_promise(&mut self) {
        let new_lines = if let Some(ref promise) = self.state.pending_log_promise {
            match promise.ready() {
                Some(lines) => Some(lines.clone()),
                None => None,
            }
        } else {
            return;
        };

        if let Some(lines) = new_lines {
            self.state.pending_log_promise = None;
            // Append new lines (dedup by checking last line to avoid double-appending)
            for line in lines {
                if self.state.log_lines.last().map(|l| l.as_str()) != Some(line.as_str()) {
                    self.state.log_lines.push(line);
                }
            }
            // Cap at 500 lines to prevent memory growth
            const MAX_LOG_LINES: usize = 500;
            if self.state.log_lines.len() > MAX_LOG_LINES {
                let drain_count = self.state.log_lines.len() - MAX_LOG_LINES;
                self.state.log_lines.drain(0..drain_count);
            }
        }
    }

    // ── Skills Tab ────────────────────────────────────────────────────────────

    fn show_skills_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let lang = self.state.language;
        let _night = self.state.night_mode;
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                if ui.selectable_label(self.state.skills_subtab == SkillsSubTab::Installed, t("skills.installed", lang)).clicked() {
                    self.state.skills_subtab = SkillsSubTab::Installed;
                }
                if ui.selectable_label(self.state.skills_subtab == SkillsSubTab::Market, t("skills.market", lang)).clicked() {
                    self.state.skills_subtab = SkillsSubTab::Market;
                    if self.state.market_skills.is_empty() && !self.state.market_loading {
                        self.do_market_search("".to_string(), 1, ctx);
                    }
                }
                if ui.selectable_label(self.state.skills_subtab == SkillsSubTab::Manual, t("skills.manual", lang)).clicked() {
                    self.state.skills_subtab = SkillsSubTab::Manual;
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("↻").on_hover_text(t("btn.refresh", lang)).clicked() {
                        self.trigger_refresh(ctx);
                        if self.state.skills_subtab == SkillsSubTab::Market {
                            self.do_market_search(self.state.market_search_query.clone(), 1, ctx);
                        }
                    }
                    if self.state.skills_promise.is_some() || self.state.market_loading {
                        ui.add_space(8.0);
                        ui.spinner();
                    }
                });
            });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(2.0); // Reduced bottom space for a tighter look

            match self.state.skills_subtab {
                SkillsSubTab::Installed => self.show_installed_skills(ui, ctx),
                SkillsSubTab::Market => self.show_market_subtab(ui, ctx),
                SkillsSubTab::Manual => self.show_manual_install_subtab(ui, ctx),
            }
        });
    }

    fn show_installed_skills(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context) {
            if self.state.skills_promise.is_some() && self.state.skills.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.add_space(40.0);
                    ui.spinner();
                    ui.label(RichText::new("Discovering skills...").color(palette::text_dim(self.state.night_mode)));
                });
                return;
            }

            if self.state.skills.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.add_space(40.0);
                    ui.label(
                        RichText::new("No skills loaded.")
                            .color(palette::text_dim(self.state.night_mode))
                            .font(FontId::new(14.0, egui::FontFamily::Monospace)),
                    );
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("Click 'Refresh' or connect to a running aimaxxing-gateway.")
                            .color(palette::text_dim(self.state.night_mode))
                            .small(),
                    );
                });
                return;
            }

            let mut toggle_target: Option<String> = None;
            let mut expand_target: Option<String> = None;
            let mut uninstall_target: Option<String> = None;

            let row_height = 38.0;
            egui::ScrollArea::vertical()
                .id_salt("installed_skills_scroll")
                .show_rows(ui, row_height, self.state.skills.len(), |ui, row_range| {
                    for i in row_range {
                        let skill = &self.state.skills[i];
                        egui::Frame::new()
                            .fill(self.theme_bg_deep())
                            .stroke(Stroke::new(1.0, palette::border(self.state.night_mode)))
                            .corner_radius(egui::CornerRadius::same(6))
                            .inner_margin(egui::Margin::symmetric(14, 8))
                            .outer_margin(egui::Margin::symmetric(0, 4))
                            .show(ui, |ui| {
                                ui.vertical(|ui| {
                                    ui.horizontal(|ui| {
                                        let status_dot = if skill.enabled { "●" } else { "○" };
                                        let status_color = if skill.enabled { palette::SUCCESS } else { palette::text_dim(self.state.night_mode) };
                                        ui.label(RichText::new(status_dot).color(status_color).small());
                                        ui.add_space(8.0); 
                                        
                                        let name_resp = ui.add(
                                            egui::Button::new(
                                                RichText::new(&skill.name)
                                                    .strong()
                                                    .color(palette::ACCENT)
                                                    .font(FontId::new(13.0, egui::FontFamily::Monospace)),
                                            ).frame(false),
                                        );
                                        if name_resp.clicked() {
                                            expand_target = Some(skill.name.clone());
                                        }

                                        if let Some(rt) = &skill.runtime {
                                            ui.add_space(6.0);
                                            ui.label(RichText::new(format!("[{}]", rt)).small().color(palette::text_dim(self.state.night_mode)));
                                        }

                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            ui.spacing_mut().item_spacing.x = 8.0;
                                            let btn_size = egui::vec2(75.0, 20.0);

                                            if ui.add_sized(
                                                btn_size,
                                                egui::Button::new(RichText::new("Details").small().color(palette::text_dim(self.state.night_mode)))
                                                    .fill(Color32::TRANSPARENT)
                                                    .stroke(Stroke::new(1.0, palette::border(self.state.night_mode)))
                                            ).clicked() {
                                                expand_target = Some(skill.name.clone());
                                            }

                                            if ui.add_sized(
                                                btn_size,
                                                egui::Button::new(RichText::new("Uninstall").small().color(palette::DANGER))
                                                    .fill(Color32::TRANSPARENT)
                                                    .stroke(Stroke::new(1.0, palette::DANGER))
                                            ).clicked() {
                                                uninstall_target = Some(skill.name.clone());
                                            }

                                            let btn_text = if skill.enabled { "Enabled" } else { "Enable" };
                                            let btn_color = if skill.enabled { palette::SUCCESS } else { palette::text_dim(self.state.night_mode) };
                                            if ui.add_sized(
                                                btn_size,
                                                egui::Button::new(RichText::new(btn_text).color(btn_color).small())
                                                    .fill(Color32::TRANSPARENT)
                                                    .stroke(Stroke::new(1.0, btn_color))
                                            ).clicked() {
                                                toggle_target = Some(skill.name.clone());
                                            }
                                        });
                                    });
                                });
                            });
                    }
                });

            if let Some(name) = expand_target { self.state.expanded_skill = Some(name); }
            if let Some(name) = toggle_target {
                if let Some(s) = self.state.skills.iter_mut().find(|s| s.name == name) { s.enabled = !s.enabled; }
                let client = self.state.client.clone();
                #[cfg(not(target_arch = "wasm32"))] spawn_task(&self.rt, async move { let _ = client.toggle_skill(&name).await; });
                #[cfg(target_arch = "wasm32")] spawn_task(async move { let _ = client.toggle_skill(&name).await; });
            }
            if let Some(name) = uninstall_target {
                self.state.skills.retain(|s| s.name != name);
                let client = self.state.client.clone();
                #[cfg(not(target_arch = "wasm32"))] spawn_task(&self.rt, async move { let _ = client.uninstall_skill(&name).await; });
                #[cfg(target_arch = "wasm32")] spawn_task(async move { let _ = client.uninstall_skill(&name).await; });
            }
    }

    fn show_market_subtab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        // Initial search if empty
        if self.state.market_skills.is_empty() && self.state.market_search_promise.is_none() && !self.state.market_loading {
             self.do_market_search("".to_string(), 1, ctx);
        }

        ui.vertical(|ui| {
            // Search Bar
            ui.horizontal(|ui| {
                ui.label("Search:");
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.state.market_search_query)
                        .hint_text("Search Smithery, GitHub, local...")
                        .desired_width(ui.available_width() - 120.0)
                );
                
                if (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) || ui.button("Search").clicked() {
                    self.do_market_search(self.state.market_search_query.clone(), 1, ctx);
                }
                if ui.button("Refresh").clicked() {
                    self.do_market_search(self.state.market_search_query.clone(), 1, ctx);
                }
            });
            ui.add_space(12.0);

            // Installation feedback
            if let Some(ref ok) = self.state.market_install_success.clone() {
                 ui.label(RichText::new(format!("✓ {}", ok)).color(palette::SUCCESS).small());
                 ui.add_space(8.0);
            }
            if let Some(ref err) = self.state.market_install_error.clone() {
                 ui.label(RichText::new(format!("✗ {}", err)).color(palette::DANGER).small());
                 ui.add_space(8.0);
            }

            if self.state.market_loading && self.state.market_page == 1 {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Searching Smithery...");
                });
            } else if let Some(err) = &self.state.market_error {
                ui.label(RichText::new(format!("Error: {}", err)).color(palette::DANGER));
            } else if self.state.market_skills.is_empty() {
                ui.label(RichText::new("No results found. Try another query.").color(palette::text_dim(self.state.night_mode)));
            } else {
                let row_height = 80.0;
                let market_skills_count = self.state.market_skills.len();
                egui::ScrollArea::vertical().id_salt("market_skills_scroll").show_rows(ui, row_height, market_skills_count + 1, |ui, row_range| {
                    for i in row_range {
                        if i < market_skills_count {
                            // Local binding to avoid borrow conflict when calling self methods inside frames
                            let (skill_name, skill_author, skill_desc, skill_url, skill_source, skill_stars) = {
                                let s = &self.state.market_skills[i];
                                (s.name.clone(), s.author.clone(), s.description.clone(), s.url.clone(), s.source.clone(), s.stars)
                            };

                            egui::Frame::new()
                                .fill(self.theme_bg_deep())
                                .stroke(Stroke::new(1.0, palette::border(self.state.night_mode)))
                                .corner_radius(egui::CornerRadius::same(6))
                                .inner_margin(egui::Margin::same(10))
                                .outer_margin(egui::Margin::symmetric(0, 3))
                                .show(ui, |ui| {
                                    let available_width = ui.available_width();
                                    let right_width = 100.0;
                                    let left_width = (available_width - right_width).max(100.0);

                                    ui.horizontal(|ui| {
                                        ui.allocate_ui(egui::vec2(left_width, 0.0), |ui| {
                                            ui.vertical(|ui| {
                                                ui.horizontal(|ui| {
                                                    ui.label(RichText::new(&skill_name).strong().color(palette::ACCENT));
                                                    ui.label(RichText::new(format!("@{}", skill_author)).small().color(palette::text_dim(self.state.night_mode)));
                                                    
                                                    let (badge_color, badge_text) = match skill_source.as_str() {
                                                        "smithery" => (palette::SUCCESS, "Smithery"),
                                                        "github" => (palette::INFO, "GitHub"),
                                                        _ => (palette::text_dim(self.state.night_mode), "Market"),
                                                    };
                                                    ui.label(RichText::new(badge_text).small().color(badge_color).strong());
                                                    
                                                    if let Some(stars) = skill_stars {
                                                        ui.label(RichText::new(format!("★ {}", stars)).small().color(palette::WARNING));
                                                    }
                                                });

                                                if !skill_desc.is_empty() {
                                                    ui.add_space(2.0);
                                                    ui.label(RichText::new(&skill_desc).small().color(palette::text_dim(self.state.night_mode)));
                                                }
                                            });
                                        });

                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            let is_installing = self.state.market_installing_url.as_deref() == Some(&skill_url);
                                            if is_installing {
                                                ui.spinner();
                                                ui.label(RichText::new("Installing...").small().color(palette::ACCENT));
                                            } else {
                                                if ui.button("Install").clicked() {
                                                    self.do_market_install(skill_url.clone(), ctx);
                                                }
                                            }
                                            ui.hyperlink_to(RichText::new("View").small(), &skill_url);
                                        });
                                    });
                                });
                        } else {
                            // "Load More" row
                            ui.add_space(20.0);
                            ui.vertical_centered(|ui| {
                                if self.state.market_loading {
                                    ui.spinner();
                                } else {
                                    if ui.button("Load More Skills").clicked() {
                                        let next_page = self.state.market_page + 1;
                                        let query = self.state.market_search_query.clone();
                                        self.do_market_search(query, next_page, ctx);
                                    }
                                }
                            });
                            ui.add_space(20.0);
                        }
                    }
                });
            }
        });
    }

    // ── Skill Detail Window ───────────────────────────────────────────────────

    fn show_skill_detail_window(&mut self, ctx: &egui::Context) {
        let skill_name = match &self.state.expanded_skill {
            Some(n) => n.clone(),
            None => return,
        };

        let skill = match self.state.skills.iter().find(|s| s.name == skill_name) {
            Some(s) => s.clone(),
            None => {
                self.state.expanded_skill = None;
                return;
            }
        };

        let mut open = true;
        egui::Window::new(&skill.name)
            .open(&mut open)
            .resizable(true)
            .min_width(480.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .frame(
                egui::Frame::new()
                    .fill(self.theme_bg_deep())
                    .stroke(Stroke::new(1.0, palette::border(self.state.night_mode)))
                    .corner_radius(egui::CornerRadius::same(8))
                    .inner_margin(egui::Margin::same(20)),
            )
            .show(ctx, |ui| {
                // Title
                ui.horizontal(|ui| {
                    let dot = if skill.enabled { "●" } else { "○" };
                    let dot_color = if skill.enabled {
                        palette::SUCCESS
                    } else {
                        palette::DANGER
                    };
                    ui.label(RichText::new(dot).color(dot_color).strong());
                    ui.label(
                        RichText::new(&skill.name)
                            .font(FontId::new(16.0, egui::FontFamily::Monospace))
                            .color(palette::text_bright(self.state.night_mode))
                            .strong(),
                    );
                });
                ui.add_space(4.0);
                ui.label(
                    RichText::new(&skill.description)
                        .color(palette::text_dim(self.state.night_mode)),
                );
                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);

                // Metadata grid
                egui::Grid::new("skill_meta_grid")
                    .num_columns(2)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| {
                        let kv = |ui: &mut egui::Ui, key: &str, val: &str| {
                            ui.label(
                                RichText::new(key).small().color(palette::text_dim(self.state.night_mode)),
                            );
                            ui.label(
                                RichText::new(val)
                                    .small()
                                    .color(palette::text_bright(self.state.night_mode))
                                    .font(FontId::new(12.0, egui::FontFamily::Monospace)),
                            );
                            ui.end_row();
                        };

                        kv(ui, "Runtime:", skill.runtime.as_deref().unwrap_or("—"));
                        kv(ui, "Kind:", &skill.kind);
                        kv(ui, "Version:", skill.version.as_deref().unwrap_or("—"));
                        kv(ui, "Author:", skill.author.as_deref().unwrap_or("—"));
                        kv(
                            ui,
                            "Status:",
                            if skill.enabled { "Enabled" } else { "Disabled" },
                        );

                        if let Some(hp) = &skill.homepage {
                            ui.label(
                                RichText::new("Homepage:").small().color(palette::text_dim(self.state.night_mode)),
                            );
                            ui.hyperlink_to(
                                RichText::new(hp).small().color(palette::ACCENT),
                                hp,
                            );
                            ui.end_row();
                        }
                    });

                // Dependencies
                if !skill.dependencies.is_empty() {
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("Dependencies:")
                            .small()
                            .color(palette::text_dim(self.state.night_mode)),
                    );
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        for dep in &skill.dependencies {
                            egui::Frame::new()
                                .fill(palette::TAG_BG)
                                .corner_radius(egui::CornerRadius::same(4))
                                .inner_margin(egui::Margin::symmetric(6, 2))
                                .show(ui, |ui| {
                                    ui.label(
                                        RichText::new(dep)
                                            .small()
                                            .color(palette::ACCENT)
                                            .font(FontId::new(
                                                11.0,
                                                egui::FontFamily::Monospace,
                                            )),
                                    );
                                });
                        }
                    });
                }
            });

        if !open {
            self.state.expanded_skill = None;
        }
    }

    // ── Vault Tab ─────────────────────────────────────────────────────────────

    fn show_api_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.vertical(|ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let is_keys = self.state.api_subtab == ApiSubTab::Keys;
                let is_voice = self.state.api_subtab == ApiSubTab::Voice;
                let is_comm = self.state.api_subtab == ApiSubTab::Comm;

                if ui.selectable_label(is_keys, t("tabs.keys", self.state.language)).clicked() {
                    self.state.api_subtab = ApiSubTab::Keys;
                }
                if ui.selectable_label(is_voice, t("tabs.speech", self.state.language)).clicked() {
                    self.state.api_subtab = ApiSubTab::Voice;
                }
                if ui.selectable_label(is_comm, t("tabs.comm", self.state.language)).clicked() {
                    self.state.api_subtab = ApiSubTab::Comm;
                }
            });
            ui.separator();
            ui.add_space(8.0);

            match self.state.api_subtab {
                ApiSubTab::Keys  => self.show_api_keys(ui, ctx),
                ApiSubTab::Voice => self.show_api_speech(ui, ctx),
                ApiSubTab::Comm  => {
                    ui.add_space(8.0);
                    self.show_channels_tab(ui, ctx);
                }
            }
        });
    }

    fn show_api_keys(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.vertical(|ui| {
            ui.set_width(ui.available_width()); // Force expansion at the tab root level
            ui.label(
                RichText::new("Credentials — API Keys")
                    .font(FontId::new(18.0, egui::FontFamily::Monospace))
                    .strong(),
            );
            ui.add_space(4.0);
            ui.label(
                RichText::new("Keys are injected as env vars — never passed as CLI args.")
                    .small()
                    .color(palette::text_dim(self.state.night_mode)),
            );
            ui.add_space(12.0);

            let n = self.state.vault_entries.len();
            let mut delete_idx: Option<usize> = None;
            let bg_color = self.theme_bg_deep();

            for i in 0..n {
                let key_name = &self.state.vault_entries[i].key;
                let is_channel_key = self.state.channel_metadata.iter()
                    .any(|meta| meta.fields.iter().any(|f| f.key.to_uppercase() == *key_name));
                
                if is_channel_key {
                    continue; // Skip channel API keys, they belong in the Communication tab
                }

                let entry = &mut self.state.vault_entries[i];

                egui::Frame::new()
                    .fill(bg_color)
                    .stroke(Stroke::new(1.0, palette::border(self.state.night_mode)))
                    .corner_radius(egui::CornerRadius::same(6))
                    .inner_margin(egui::Margin::symmetric(12, 12))
                    .outer_margin(egui::Margin::symmetric(0, 0)) // Remove side margin to fill right
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.vertical(|ui| {
                            ui.label(
                                RichText::new(&entry.key)
                                    .font(FontId::new(13.0, egui::FontFamily::Monospace))
                                    .color(palette::ACCENT),
                            );

                            ui.horizontal(|ui| {
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    // 1. Right-side Buttons
                                    let standard = ["OPENAI_API_KEY", "ANTHROPIC_API_KEY", "GEMINI_API_KEY", "DEEPSEEK_API_KEY", "MINIMAX_API_KEY"];
                                    if !standard.contains(&entry.key.as_str()) {
                                        if ui.button(RichText::new("🗑").color(palette::DANGER)).clicked() {
                                            let client = self.state.client.clone();
                                            let key = entry.key.clone();
                                            let ctx2 = ctx.clone();
                                            #[cfg(not(target_arch = "wasm32"))]
                                            spawn_task(&self.rt, async move {
                                                let _ = client.delete_vault_secret(&key).await;
                                                ctx2.request_repaint();
                                            });

                                            self.state.deleted_vault_keys.insert(entry.key.clone());
                                            delete_idx = Some(i);
                                        }
                                    }

                                    if ui.small_button("💾 Save").clicked() && !entry.value.is_empty() {
                                        let client = self.state.client.clone();
                                        let key = entry.key.clone();
                                        let val = entry.value.clone();
                                        entry.error = None;
                                        let ctx2 = ctx.clone();

                                        #[cfg(not(target_arch = "wasm32"))]
                                        spawn_task(&self.rt, async move {
                                            if let Ok(_) = client.save_vault_secret(&key, &val).await {
                                                ctx2.request_repaint();
                                            }
                                        });

                                        entry.value.clear();
                                        entry.saved = true;
                                    }

                                    // 2. Input takes ALL REMAINING space to the left
                                    let pw = egui::TextEdit::singleline(&mut entry.value)
                                        .password(!self.state.vault_show_value)
                                        .hint_text("Enter key value…")
                                        .font(FontId::new(12.0, egui::FontFamily::Monospace))
                                        .desired_width(ui.available_width());
                                    ui.add(pw);
                                });
                            });
                        });
                    });
            }

            // 执行延迟删除
            if let Some(idx) = delete_idx {
                self.state.vault_entries.remove(idx);
            }

            // If any save/delete was clicked (indicated by 0.0), trigger refresh
            if self.state.last_snapshot_refresh_time == 0.0 {
                self.state.last_snapshot_refresh_time = ctx.input(|i| i.time);
                self.do_snapshot_refresh(ctx);
            }

            ui.add_space(16.0);
            ui.separator();
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                let show_lbl = if self.state.vault_show_value {
                    "Hide values"
                } else {
                    "Show values"
                };
                if ui.small_button(show_lbl).clicked() {
                    self.state.vault_show_value = !self.state.vault_show_value;
                }
            });

            ui.add_space(16.0);
            ui.label(
                RichText::new("Add Custom Key")
                    .color(palette::text_dim(self.state.night_mode))
                    .small(),
            );
            ui.horizontal(|ui| {
                let avg_width = (ui.available_width() - 80.0) / 2.0;
                ui.add(
                    egui::TextEdit::singleline(&mut self.state.new_vault_key)
                        .hint_text("KEY_NAME")
                        .font(FontId::new(12.0, egui::FontFamily::Monospace))
                        .desired_width(avg_width.max(120.0)),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut self.state.new_vault_value)
                        .hint_text("value")
                        .password(true)
                        .font(FontId::new(12.0, egui::FontFamily::Monospace))
                        .desired_width(avg_width.max(160.0)),
                );
                if ui.button("Add").clicked() && !self.state.new_vault_key.is_empty() {
                    let key_raw = self.state.new_vault_key.drain(..).collect::<String>();
                    let val = self.state.new_vault_value.drain(..).collect::<String>();

                    let mut key = key_raw.trim().to_uppercase();
                    if !key.ends_with("_API_KEY") {
                        key = format!("{}_API_KEY", key);
                    }

                    self.state.deleted_vault_keys.remove(&key);

                    let idx = self.state.vault_entries.len();
                    self.state.vault_entries.push(VaultEntry {
                        key: key.clone(),
                        value: String::new(),
                        saved: false,
                        ..Default::default()
                    });

                    let client = self.state.client.clone();
                    let ctx2 = ctx.clone();
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        let rt = self.rt.clone();
                        spawn_task(&rt, async move {
                            let _ = client.save_vault_secret(&key, &val).await;
                            ctx2.request_repaint();
                        });
                    }
                    #[cfg(target_arch = "wasm32")]
                    spawn_task(async move {
                        let _ = client.save_vault_secret(&key, &val).await;
                        ctx2.request_repaint();
                    });

                    if let Some(e) = self.state.vault_entries.get_mut(idx) {
                        e.saved = true;
                    }
                }
            });
        });
    }

    fn show_api_speech(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context) {
        let lang = self.state.language;
        let night = self.state.night_mode;

        ui.vertical(|ui| {
            ui.heading(RichText::new(t("speech.title", lang)).strong().color(palette::text_bright(night)));
            ui.add_space(4.0);
            ui.label(RichText::new("Globally configure Voice synthesis (TTS) and recognition logic for the entire swarm.").small().color(palette::text_dim(night)));
            ui.add_space(16.0);

            // ── Section: OpenAI TTS ──────────────────────────────────────────
            egui::Frame::new()
                .fill(self.theme_bg_deep())
                .stroke(Stroke::new(1.0, palette::border(night)))
                .corner_radius(egui::CornerRadius::same(8))
                .inner_margin(egui::Margin::same(16))
                .show(ui, |ui| {
                    ui.label(RichText::new(t("speech.openai_tts", lang)).strong().color(palette::ACCENT));
                    ui.add_space(12.0);

                    // Model Selection
                    ui.horizontal(|ui| {
                        ui.label(format!("{}:", t("speech.model", lang)));
                        let models = ["tts-1", "tts-1-hd"];
                        for m in models {
                            let is_sel = self.state.voice_tts_model == m;
                            if ui.selectable_label(is_sel, m).clicked() {
                                self.state.voice_tts_model = m.to_string();
                                self.state.set_status("Speech settings updated", false);
                                crate::app_state::save_config(&self.state);
                            }
                        }
                    });
                    ui.add_space(8.0);

                    // Voice Selection
                    ui.horizontal(|ui| {
                        ui.label(format!("{}:", t("speech.voice", lang)));
                        let voices = ["alloy", "echo", "fable", "onyx", "nova", "shimmer"];
                        ui.horizontal_wrapped(|ui| {
                            for v in voices {
                                let is_sel = self.state.voice_tts_voice == v;
                                if ui.selectable_label(is_sel, v).clicked() {
                                    self.state.voice_tts_voice = v.to_string();
                                    self.state.set_status("Speech settings updated", false);
                                    crate::app_state::save_config(&self.state);
                                }
                            }
                        });
                    });
                });

            ui.add_space(16.0);

            // ── Section: Local Voice Model ───────────────────────────────────
            egui::Frame::new()
                .fill(self.theme_bg_deep())
                .stroke(Stroke::new(1.0, palette::border(night)))
                .corner_radius(egui::CornerRadius::same(8))
                .inner_margin(egui::Margin::same(16))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(t("speech.local_model", lang)).strong().color(palette::ACCENT));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.checkbox(&mut self.state.voice_local_tts_enabled, t("speech.enabled", lang)).changed() {
                                self.state.set_status("Local speech toggled", false);
                                crate::app_state::save_config(&self.state);
                            }
                        });
                    });
                    ui.add_space(12.0);

                    ui.label(format!("{}:", t("speech.path", lang)));
                    let path_edit = egui::TextEdit::singleline(&mut self.state.voice_local_tts_path)
                        .hint_text("/path/to/local/model.bin")
                        .desired_width(ui.available_width());
                    if ui.add(path_edit).changed() {
                        self.state.set_status("Local model path updated", false);
                        crate::app_state::save_config(&self.state);
                    }
                    ui.add_space(4.0);
                    ui.label(RichText::new("Path to GGUF/ONNX voice model for local synthesis.").small().color(palette::text_dim(night)));
                });
            
            ui.add_space(24.0);
            
            ui.horizontal(|ui| {
                if ui.button(RichText::new("🔄 Sync Global Voice to Gateway").strong()).clicked() {
                    let client = self.state.client.clone();
                    let model = self.state.voice_tts_model.clone();
                    let voice = self.state.voice_tts_voice.clone();
                    let local_en = if self.state.voice_local_tts_enabled { "true" } else { "false" }.to_string();
                    let local_path = self.state.voice_local_tts_path.clone();
                    let ctx2 = _ctx.clone();

                    #[cfg(not(target_arch = "wasm32"))]
                    spawn_task(&self.rt, async move {
                        let _ = client.save_vault_secret("VOICE_TTS_MODEL", &model).await;
                        let _ = client.save_vault_secret("VOICE_TTS_VOICE", &voice).await;
                        let _ = client.save_vault_secret("VOICE_LOCAL_TTS_ENABLED", &local_en).await;
                        let _ = client.save_vault_secret("VOICE_LOCAL_TTS_PATH", &local_path).await;
                        ctx2.request_repaint();
                    });
                    #[cfg(target_arch = "wasm32")]
                    spawn_task(async move {
                        let _ = client.save_vault_secret("VOICE_TTS_MODEL", &model).await;
                        let _ = client.save_vault_secret("VOICE_TTS_VOICE", &voice).await;
                        let _ = client.save_vault_secret("VOICE_LOCAL_TTS_ENABLED", &local_en).await;
                        let _ = client.save_vault_secret("VOICE_LOCAL_TTS_PATH", &local_path).await;
                        ctx2.request_repaint();
                    });
                    self.state.set_status("Voice settings committed to Gateway Vault", false);
                }
                ui.label(RichText::new("💡 Note: Committing to Vault makes these settings visible to all Agents and Tools.").small().color(palette::text_dim(night)));
            });
        });
    }

    // ── Logs Tab ──────────────────────────────────────────────────────────────

    fn show_logs_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        let next_poll_in = (2.0 - (now - self.state.last_log_poll_time)).max(0.0);

        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Audit Logs")
                        .font(FontId::new(18.0, egui::FontFamily::Monospace))
                        .strong(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Manual trigger
                    if ui.button("▶ Poll Now").clicked() {
                        self.state.last_log_poll_time = now;
                        self.do_log_poll(ctx);
                    }
                    
                    ui.add_space(12.0); // Space between Poll and Auto-ON

                    if self.state.auto_log_poll {
                        ui.label(
                            RichText::new(format!("next: {:.0}s", next_poll_in))
                                .small()
                                .color(palette::text_dim(self.state.night_mode)),
                        );
                    }
                    
                    // Auto-refresh toggle
                    let auto_label = if self.state.auto_log_poll {
                        RichText::new("⏱ Auto ON").small().color(palette::SUCCESS)
                    } else {
                        RichText::new("⏱ Auto OFF").small().color(palette::text_dim(self.state.night_mode))
                    };
                    if ui.add(egui::Button::new(auto_label).fill(Color32::TRANSPARENT)).clicked() {
                        self.state.auto_log_poll = !self.state.auto_log_poll;
                    }

                    ui.add_space(12.0); // Space between Auto-ON and Clear

                    if ui.small_button("✕ Clear").clicked() {
                        self.state.log_lines.clear();
                    }
                });
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Firewall intercepts & skill execution events.")
                        .small()
                        .color(palette::text_dim(self.state.night_mode)),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("{} entries", self.state.log_lines.len()))
                            .small()
                            .color(palette::text_dim(self.state.night_mode)),
                    );
                });
            });
            ui.add_space(8.0);

            egui::Frame::new()
                .fill(self.theme_bg_deep())
                .stroke(Stroke::new(1.0, palette::border(self.state.night_mode)))
                .corner_radius(egui::CornerRadius::ZERO) // No corner rounding for better 'flush' look
                .inner_margin(egui::Margin::symmetric(14, 10)) // Some side padding for text readability inside
                .outer_margin(egui::Margin::symmetric(0, 0)) // Truly flush with edges
                .show(ui, |ui| {
                    egui::ScrollArea::both()
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            if self.state.log_lines.is_empty() {
                                ui.label(
                                    RichText::new(
                                        "No log entries yet.\nEnable 'Auto ON' or click '▶ Poll Now' to fetch from gateway.",
                                    )
                                    .small()
                                    .color(palette::text_dim(self.state.night_mode)),
                                );
                            } else {
                                // Logs are capped at 500 lines, so rendering them directly is very fast.
                                // Using `show_rows` with dynamic text wrapping caused severe layout loop lag.
                                for line in &self.state.log_lines {
                                    let color = if line.contains("ERROR")
                                        || line.contains("BLOCKED")
                                    {
                                        palette::DANGER
                                    } else if line.contains("WARN") {
                                        Color32::from_rgb(251, 146, 60)
                                    } else {
                                        palette::text_dim(self.state.night_mode)
                                    };
                                    ui.label(
                                        RichText::new(line)
                                            .small()
                                            .color(color)
                                            .font(FontId::new(
                                                11.0,
                                                egui::FontFamily::Monospace,
                                            )),
                                    );
                                }
                            }
                        });
                });
        });
    }

    /// Shared log-polling logic called by both the timer and the manual button.
    fn do_log_poll(&mut self, ctx: &egui::Context) {
        let client = self.state.client.clone();
        let ctx2 = ctx.clone();

        // We use a Promise to pipe results back into state on the next frame
        let (sender, promise) = Promise::<Vec<String>>::new();

        #[cfg(not(target_arch = "wasm32"))]
        spawn_task(&self.rt, async move {
            let lines = client.poll_logs().await.unwrap_or_default();
            sender.send(lines);
            ctx2.request_repaint();
        });
        #[cfg(target_arch = "wasm32")]
        spawn_task(async move {
            let lines = client.poll_logs().await.unwrap_or_default();
            sender.send(lines);
            ctx2.request_repaint();
        });

        // Poll the promise immediately next frame
        // We store it temporarily and apply in the state update
        // Simple approach: store a one-shot log promise
        self.state.pending_log_promise = Some(promise);
    }

    // ── Store Tab ─────────────────────────────────────────────────────────────

    fn show_manual_install_subtab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.vertical(|ui| {
            // ── Header ──────────────────────────────────────────────────────
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Manual Installation")
                        .font(FontId::new(16.0, egui::FontFamily::Monospace))
                        .color(palette::text_bright(self.state.night_mode))
                        .strong(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new("🌐 Browse skills.sh →")
                                    .small()
                                    .color(palette::ACCENT),
                            )
                            .fill(Color32::TRANSPARENT)
                            .stroke(Stroke::new(1.0, palette::ACCENT)),
                        )
                        .on_hover_text("Open skills.sh in your browser to discover skills")
                        .clicked()
                    {
                        ctx.open_url(egui::OpenUrl {
                            url: "https://skills.sh".to_string(),
                            new_tab: true,
                        });
                    }
                });
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new(
                    "Visit skills.sh, find a skill, then copy the full install command shown on the page and paste it below.",
                )
                .small()
                .color(palette::text_dim(self.state.night_mode)),
            );
            ui.add_space(14.0);

            // ── Install from URL ─────────────────────────────────────────────
            egui::Frame::new()
                .fill(self.theme_bg_deep())
                .stroke(Stroke::new(1.0, palette::border(self.state.night_mode)))
                .corner_radius(egui::CornerRadius::same(8))
                .inner_margin(egui::Margin::same(14))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new("Install Skill")
                            .strong()
                            .color(palette::ACCENT),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("Paste the install command from skills.sh, or a GitHub / skills.sh URL:")
                            .small()
                            .color(palette::text_dim(self.state.night_mode)),
                    );
                    ui.add_space(8.0);

                    // URL input
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("URL").small().color(palette::text_dim(self.state.night_mode)));
                        ui.add_space(8.0);
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut self.state.store_install_url)
                                .hint_text(
                                    "npx skills add https://github.com/aimaxxing-labs/skills --skill find-skills",
                                )
                                .desired_width(f32::INFINITY),
                        );
                        // Install on Enter key
                        if resp.lost_focus()
                            && ui.input(|i| i.key_pressed(egui::Key::Enter))
                            && !self.state.store_installing
                        {
                            self.do_install_skill(ctx);
                        }
                    });
                    ui.add_space(8.0);

                    ui.horizontal(|ui| {
                        let btn = ui.add_enabled(
                            !self.state.store_installing
                                && !self.state.store_install_url.is_empty(),
                            egui::Button::new(
                                RichText::new(if self.state.store_installing {
                                    "  Installing…  "
                                } else {
                                    "  ↓ Install  "
                                })
                                .strong(),
                            ),
                        );
                        if btn.clicked() {
                            self.do_install_skill(ctx);
                        }

                        if self.state.store_installing {
                            ui.spinner();
                        }

                        // Quick-copy examples
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            for (label, url) in &[
                                ("find-skills", "https://github.com/aimaxxing-labs/skills/tree/main/skills/find-skills"),
                                ("rust-daily", "https://github.com/biubiuboy/.agent/tree/main/skills/rust-daily"),
                            ] {
                                if ui
                                    .add(
                                        egui::Button::new(
                                            RichText::new(format!("▸ {}", label))
                                                .small()
                                                .color(palette::text_dim(self.state.night_mode)),
                                        )
                                        .fill(Color32::TRANSPARENT),
                                    )
                                    .on_hover_text(format!("Use: {}", url))
                                    .clicked()
                                {
                                    self.state.store_install_url = url.to_string();
                                }
                            }
                        });
                    });

                    ui.add_space(4.0);

                    // Status / error / success feedback
                    if let Some(ref err) = self.state.store_install_error.clone() {
                        ui.label(
                            RichText::new(format!("✗  {}", err))
                                .small()
                                .color(palette::DANGER),
                        );
                    }
                    if let Some(ref ok) = self.state.store_install_success.clone() {
                        ui.label(
                            RichText::new(format!("✓  {}", ok))
                                .small()
                                .color(palette::SUCCESS),
                        );
                    }
                });
        });
    }

    fn do_install_skill(&mut self, ctx: &egui::Context) {
        let url = self.state.store_install_url.trim().to_string();
        if url.is_empty() { return; }

        self.state.store_installing = true;
        self.state.store_install_error = None;
        self.state.store_install_success = None;

        let client = self.state.client.clone();
        let ctx2 = ctx.clone();
        let (sender, promise) = Promise::new();

        #[cfg(not(target_arch = "wasm32"))]
        spawn_task(&self.rt, async move {
            let result = match client.install_skill(&url).await {
                Ok(res) => {
                    let _ = client.toggle_skill(&res.skill_name).await;
                    Ok(res)
                }
                Err(e) => Err(e.to_string()),
            };
            sender.send(result);
            ctx2.request_repaint();
        });
        #[cfg(target_arch = "wasm32")]
        spawn_task(async move {
            let result = match client.install_skill(&url).await {
                Ok(res) => {
                    let _ = client.toggle_skill(&res.skill_name).await;
                    Ok(res)
                }
                Err(e) => Err(e.to_string()),
            };
            sender.send(result);
            ctx2.request_repaint();
        });

        self.state.pending_install_promise = Some(promise);
    }


    // ── Connection Tab ────────────────────────────────────────────────────────

    fn show_connection_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.vertical(|ui| {
            ui.label(
                RichText::new("Connection Settings")
                    .font(FontId::new(18.0, egui::FontFamily::Monospace))
                    .strong(),
            );
            ui.add_space(16.0);

            egui::Frame::new()
                .fill(self.theme_bg_deep())
                .stroke(Stroke::new(1.0, palette::border(self.state.night_mode)))
                .corner_radius(egui::CornerRadius::same(8))
                .inner_margin(egui::Margin::same(16))
                .show(ui, |ui| {
                    ui.label(RichText::new("Gateway URL").color(palette::ACCENT));
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("Local:   http://localhost:3000")
                            .small()
                            .color(palette::text_dim(self.state.night_mode)),
                    );
                    ui.label(
                        RichText::new("Remote:  https://your-name.ts.net  (Tailscale)")
                            .small()
                            .color(palette::text_dim(self.state.night_mode)),
                    );
                    ui.add_space(8.0);

                    let mut url_buf = self.state.gateway_url.clone();
                    let changed = ui
                        .add(
                            egui::TextEdit::singleline(&mut url_buf)
                                .hint_text("http://localhost:3000")
                                .font(FontId::new(13.0, egui::FontFamily::Monospace))
                                .desired_width(f32::INFINITY),
                        )
                        .changed();

                    if changed {
                        self.state.gateway_url = url_buf;
                    }

                    ui.add_space(8.0);
                    if ui.button("  Connect  ").clicked() {
                        let url = self.state.gateway_url.clone();
                        self.state.set_url(url);
                        self.trigger_refresh(ctx);
                    }
                });

            ui.add_space(16.0);

            egui::Frame::new()
                .fill(self.theme_bg_deep())
                .stroke(Stroke::new(1.0, palette::border(self.state.night_mode)))
                .corner_radius(egui::CornerRadius::same(8))
                .inner_margin(egui::Margin::same(16))
                .show(ui, |ui| {
                    ui.label(RichText::new("Tailscale Setup").color(palette::ACCENT));
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(
                            "On your cloud host, run:\n\n  tailscale funnel 3000\n\nThen connect this panel to:\n  https://your-hostname.ts.net",
                        )
                        .small()
                        .color(palette::text_dim(self.state.night_mode)),
                    );
                });

            ui.add_space(16.0);

            let (status_text, status_color) = match self.state.connected {
                Some(true) => ("● Connected", palette::SUCCESS),
                Some(false) => ("● Disconnected — check gateway URL", palette::DANGER),
                None => ("○ Not checked", palette::text_dim(self.state.night_mode)),
            };
            ui.label(RichText::new(status_text).color(status_color));

            // Show snapshot info if available
            if let Some(snap) = &self.state.snapshot {
                ui.add_space(12.0);
                egui::Frame::new()
                    .fill(self.theme_bg_deep())
                    .stroke(Stroke::new(1.0, palette::border(self.state.night_mode)))
                    .corner_radius(egui::CornerRadius::same(6))
                    .inner_margin(egui::Margin::same(12))
                    .show(ui, |ui| {
                        ui.label(RichText::new("Gateway Snapshot").color(palette::ACCENT));
                        ui.add_space(6.0);
                        egui::Grid::new("snap_grid").num_columns(2).spacing([16.0, 4.0]).show(ui, |ui| {
                            ui.label(RichText::new("Version").color(palette::text_dim(self.state.night_mode)).small());
                            ui.label(RichText::new(&snap.version).small());
                            ui.end_row();
                            ui.label(RichText::new("Agents").color(palette::text_dim(self.state.night_mode)).small());
                            ui.label(RichText::new(snap.agent_count.to_string()).small());
                            ui.end_row();
                            ui.label(RichText::new("Skills").color(palette::text_dim(self.state.night_mode)).small());
                            ui.label(RichText::new(self.state.skills.len().to_string()).small());
                            ui.end_row();
                            ui.label(RichText::new("Cron Jobs").color(palette::text_dim(self.state.night_mode)).small());
                            ui.label(RichText::new(self.state.cron_jobs.len().to_string()).small());
                            ui.end_row();
                        });
                        ui.add_space(6.0);
                        for c in &snap.connectors {
                            let (icon, color) = if c.configured {
                                ("●", palette::SUCCESS)
                            } else {
                                ("○", palette::text_dim(self.state.night_mode))
                            };
                            ui.label(RichText::new(format!("{} {}", icon, c.name)).small().color(color));
                        }
                    });
            }
        });
    }

    // ── Sessions Tab ──────────────────────────────────────────────────────────

    fn show_sessions_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Active Sessions")
                        .font(FontId::new(18.0, egui::FontFamily::Monospace))
                        .strong(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("↻ Refresh").clicked() {
                        self.state.last_sessions_refresh_time = -999.0;
                        self.do_sessions_refresh(ctx);
                    }
                    ui.label(
                        RichText::new(format!("{} sessions", self.state.sessions.len()))
                            .small()
                            .color(palette::text_dim(self.state.night_mode)),
                    );
                });
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new("Conversation sessions currently active in the gateway.")
                    .small()
                    .color(palette::text_dim(self.state.night_mode)),
            );
            ui.add_space(8.0);

            if let Some(err) = &self.state.sessions_error.clone() {
                ui.label(RichText::new(err).color(palette::DANGER).small());
                ui.add_space(8.0);
            }

            if self.state.sessions_loading {
                ui.label(RichText::new("Loading…").color(palette::text_dim(self.state.night_mode)).small());
                return;
            }

            if self.state.sessions.is_empty() {
                egui::Frame::new()
                    .fill(self.theme_bg_deep())
                    .stroke(Stroke::new(1.0, palette::border(self.state.night_mode)))
                    .corner_radius(egui::CornerRadius::same(6))
                    .inner_margin(egui::Margin::same(16))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new("No active sessions.\nConversations will appear here when agents are engaged.")
                                .color(palette::text_dim(self.state.night_mode))
                                .small(),
                        );
                    });
                return;
            }

            egui::ScrollArea::vertical().show(ui, |ui| {
                let sessions = self.state.sessions.clone();
                for sess in &sessions {
                    egui::Frame::new()
                        .fill(self.theme_bg_deep())
                        .stroke(Stroke::new(1.0, palette::border(self.state.night_mode)))
                        .corner_radius(egui::CornerRadius::same(6))
                        .inner_margin(egui::Margin::same(12))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(&sess.id)
                                        .font(FontId::new(12.0, egui::FontFamily::Monospace))
                                        .color(palette::text_bright(self.state.night_mode)),
                                );
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    let id = sess.id.clone();
                                    if ui.small_button("✕ End").clicked() {
                                        let client = self.state.client.clone();
                                        let ctx2 = ctx.clone();
                                        #[cfg(not(target_arch = "wasm32"))]
                                        spawn_task(&self.rt, async move {
                                            let _ = client.delete_session(&id).await;
                                            ctx2.request_repaint();
                                        });
                                        #[cfg(target_arch = "wasm32")]
                                        spawn_task(async move {
                                            let _ = client.delete_session(&id).await;
                                            ctx2.request_repaint();
                                        });
                                        self.state.last_sessions_refresh_time = -999.0;
                                    }
                                    ui.label(
                                        RichText::new(&sess.agent_role)
                                            .small()
                                            .color(palette::ACCENT),
                                    );
                                });
                            });
                        });
                    ui.add_space(4.0);
                }
            });
        });
    }

    // ── Cron Tab ──────────────────────────────────────────────────────────────

    fn show_cron_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.vertical(|ui| {
            // Header
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Cron Scheduler")
                        .font(FontId::new(18.0, egui::FontFamily::Monospace))
                        .strong(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("↻ Refresh").clicked() {
                        self.state.last_cron_refresh_time = -999.0;
                        self.do_cron_refresh(ctx);
                    }
                    ui.label(
                        RichText::new(format!("{} jobs", self.state.cron_jobs.len()))
                            .small()
                            .color(palette::text_dim(self.state.night_mode)),
                    );
                });
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new("Schedule recurring agent tasks.")
                    .small()
                    .color(palette::text_dim(self.state.night_mode)),
            );
            ui.add_space(12.0);

            if let Some(err) = &self.state.cron_error.clone() {
                ui.label(RichText::new(err).color(palette::DANGER).small());
                ui.add_space(6.0);
            }

            // New job form
            egui::Frame::new()
                .fill(self.theme_bg_deep())
                .stroke(Stroke::new(1.0, palette::border(self.state.night_mode)))
                .corner_radius(egui::CornerRadius::same(8))
                .inner_margin(egui::Margin::same(14))
                .show(ui, |ui| {
                    ui.label(RichText::new("New Job").color(palette::ACCENT).strong());
                    ui.add_space(8.0);

                    egui::Grid::new("cron_form").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                        ui.label(RichText::new("Name").color(palette::text_dim(self.state.night_mode)).small());
                        ui.add(egui::TextEdit::singleline(&mut self.state.cron_form_name)
                            .hint_text("e.g. Daily Digest")
                            .desired_width(240.0));
                        ui.end_row();

                        ui.label(RichText::new("Schedule").color(palette::text_dim(self.state.night_mode)).small());
                        ui.horizontal(|ui| {
                            for kind in ["every", "cron"] {
                                let active = self.state.cron_form_schedule == kind;
                                if ui.selectable_label(active, kind).clicked() {
                                    self.state.cron_form_schedule = kind.to_string();
                                }
                            }
                        });
                        ui.end_row();

                        if self.state.cron_form_schedule == "every" {
                            ui.label(RichText::new("Interval (sec)").color(palette::text_dim(self.state.night_mode)).small());
                            ui.add(egui::TextEdit::singleline(&mut self.state.cron_form_interval)
                                .hint_text("3600")
                                .desired_width(100.0));
                        } else {
                            ui.label(RichText::new("Cron Expr").color(palette::text_dim(self.state.night_mode)).small());
                            ui.add(egui::TextEdit::singleline(&mut self.state.cron_form_expr)
                                .hint_text("0 9 * * *")
                                .desired_width(160.0));
                        }
                        ui.end_row();

                        ui.label(RichText::new("Prompt").color(palette::text_dim(self.state.night_mode)).small());
                        ui.add(egui::TextEdit::singleline(&mut self.state.cron_form_prompt)
                            .hint_text("What should the agent do?")
                            .desired_width(320.0));
                        ui.end_row();
                    });

                    ui.add_space(8.0);
                    if ui.button("  + Add Job  ").clicked() {
                        self.submit_cron_job(ctx);
                    }
                });

            ui.add_space(12.0);

            // Job list
            if self.state.cron_loading {
                ui.label(RichText::new("Loading…").color(palette::text_dim(self.state.night_mode)).small());
                return;
            }

            if self.state.cron_jobs.is_empty() {
                ui.label(RichText::new("No scheduled jobs yet.").color(palette::text_dim(self.state.night_mode)).small());
                return;
            }

            egui::ScrollArea::vertical().show(ui, |ui| {
                let jobs = self.state.cron_jobs.clone();
                for job in &jobs {
                    egui::Frame::new()
                        .fill(self.theme_bg_deep())
                        .stroke(Stroke::new(1.0, palette::border(self.state.night_mode)))
                        .corner_radius(egui::CornerRadius::same(6))
                        .inner_margin(egui::Margin::same(12))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let status_color = if job.enabled { palette::SUCCESS } else { palette::DANGER };
                                ui.label(RichText::new(if job.enabled { "●" } else { "○" }).color(status_color));
                                ui.label(RichText::new(&job.name).color(palette::text_bright(self.state.night_mode)).strong());
                                ui.label(RichText::new(&job.payload_kind).small().color(palette::text_dim(self.state.night_mode)));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    let job_id = job.id.clone();
                                    let client = self.state.client.clone();
                                    let ctx2 = ctx.clone();
                                    if ui.small_button("🗑 Delete").clicked() {
                                        let id = job_id.clone();
                                        let c = client.clone();
                                        let ctx3 = ctx2.clone();
                                        
                                        self.state.cron_loading = true;
                                        let (sender, promise) = poll_promise::Promise::new();
                                        self.state.pending_cron_action_promise = Some(promise);
                                        
                                        #[cfg(not(target_arch = "wasm32"))]
                                        spawn_task(&self.rt, async move {
                                            match c.delete_cron_job(&id).await {
                                                Ok(_) => sender.send(Ok("Cron job deleted.".into())),
                                                Err(e) => sender.send(Err(e.to_string())),
                                            }
                                            ctx3.request_repaint();
                                        });
                                        #[cfg(target_arch = "wasm32")]
                                        spawn_task(async move {
                                            match c.delete_cron_job(&id).await {
                                                Ok(_) => sender.send(Ok("Cron job deleted.".into())),
                                                Err(e) => sender.send(Err(e.to_string())),
                                            }
                                            ctx3.request_repaint();
                                        });
                                    }
                                    if ui.small_button("▶ Run").clicked() {
                                        let id = job_id.clone();
                                        let c = client.clone();
                                        let ctx3 = ctx2.clone();
                                        
                                        self.state.cron_loading = true;
                                        let (sender, promise) = poll_promise::Promise::new();
                                        self.state.pending_cron_action_promise = Some(promise);
                                        
                                        #[cfg(not(target_arch = "wasm32"))]
                                        spawn_task(&self.rt, async move {
                                            match c.run_cron_job(&id).await {
                                                Ok(_) => sender.send(Ok("Cron job execution triggered.".into())),
                                                Err(e) => sender.send(Err(e.to_string())),
                                            }
                                            ctx3.request_repaint();
                                        });
                                        #[cfg(target_arch = "wasm32")]
                                        spawn_task(async move {
                                            match c.run_cron_job(&id).await {
                                                Ok(_) => sender.send(Ok("Cron job execution triggered.".into())),
                                                Err(e) => sender.send(Err(e.to_string())),
                                            }
                                            ctx3.request_repaint();
                                        });
                                    }
                                });
                            });
                            ui.add_space(4.0);
                            let sched_str = serde_json::to_string(&job.schedule).unwrap_or_default();
                            ui.label(RichText::new(&sched_str).small().color(palette::text_dim(self.state.night_mode))
                                .font(FontId::new(10.0, egui::FontFamily::Monospace)));
                            if let Some(last) = &job.last_run_at {
                                ui.label(RichText::new(format!("Last: {}", last)).small().color(palette::text_dim(self.state.night_mode)));
                            }
                            if job.error_count > 0 {
                                ui.label(RichText::new(format!("⚠ {} errors", job.error_count))
                                    .small().color(Color32::from_rgb(251, 146, 60)));
                            }
                        });
                    ui.add_space(4.0);
                }
            });
        });
    }

    fn submit_cron_job(&mut self, ctx: &egui::Context) {
        use crate::api::CreateCronJobRequest;

        let name = self.state.cron_form_name.trim().to_string();
        let prompt = self.state.cron_form_prompt.trim().to_string();

        if name.is_empty() || prompt.is_empty() {
             self.state.set_status("Name and Prompt are required to create a job.".to_string(), true);
             return;
        }

        let req = CreateCronJobRequest {
            name,
            schedule_kind: self.state.cron_form_schedule.clone(),
            interval_secs: if self.state.cron_form_schedule == "every" {
                self.state.cron_form_interval.parse().ok()
            } else { None },
            cron_expr: if self.state.cron_form_schedule == "cron" {
                Some(self.state.cron_form_expr.clone())
            } else { None },
            at: None,
            prompt: Some(prompt),
        };

        let client = self.state.client.clone();
        let ctx2 = ctx.clone();
        
        // Let the polling system know we are doing an action
        self.state.cron_loading = true;
        let (sender, promise) = poll_promise::Promise::new();
        self.state.pending_cron_action_promise = Some(promise);
        
        #[cfg(not(target_arch = "wasm32"))]
        spawn_task(&self.rt, async move {
            match client.create_cron_job(req).await {
                Ok(_) => sender.send(Ok("Cron job added successfully.".into())),
                Err(e) => sender.send(Err(e.to_string())),
            }
            ctx2.request_repaint();
        });
        
        #[cfg(target_arch = "wasm32")]
        spawn_task(async move {
            match client.create_cron_job(req).await {
                Ok(_) => sender.send(Ok("Cron job added successfully.".into())),
                Err(e) => sender.send(Err(e.to_string())),
            }
            ctx2.request_repaint();
        });
    }

    // ── Async refresh helpers ─────────────────────────────────────────────────
    
    fn poll_cron_action_promise(&mut self, _ctx: &egui::Context) {
        if let Some(ref p) = self.state.pending_cron_action_promise {
            if let Some(res) = p.ready() {
                self.state.cron_loading = false;
                match res {
                    Ok(msg) => {
                        self.state.set_status(msg.clone(), false);
                        self.state.cron_form_name.clear();
                        self.state.cron_form_prompt.clear();
                        // Trigger an immediate list refresh in the next frame
                        self.state.last_cron_refresh_time = -999.0;
                    }
                    Err(e) => {
                        self.state.set_status(format!("Error: {}", e), true);
                    }
                }
                self.state.pending_cron_action_promise = None;
            }
        }
    }

    fn do_sessions_refresh(&mut self, ctx: &egui::Context) {
        if self.state.pending_sessions_promise.is_some() { return; }
        self.state.sessions_loading = true;
        let client = self.state.client.clone();
        let ctx2 = ctx.clone();
        let (sender, promise) = Promise::new();
        #[cfg(not(target_arch = "wasm32"))]
        spawn_task(&self.rt, async move {
            let result = client.list_sessions().await.map_err(|e| e.to_string());
            sender.send(result);
            ctx2.request_repaint();
        });
        #[cfg(target_arch = "wasm32")]
        spawn_task(async move {
            let result = client.list_sessions().await.map_err(|e| e.to_string());
            sender.send(result);
            ctx2.request_repaint();
        });
        self.state.pending_sessions_promise = Some(promise);
    }

    fn poll_sessions_promise(&mut self) {
        if let Some(ref p) = self.state.pending_sessions_promise {
            if let Some(result) = p.ready() {
                self.state.sessions_loading = false;
                match result {
                    Ok(sessions) => {
                        self.state.sessions = sessions.clone();
                        self.state.sessions_error = None;
                    }
                    Err(e) => {
                        self.state.sessions_error = Some(e.clone());
                    }
                }
                self.state.pending_sessions_promise = None;
            }
        }
    }

    fn do_cron_refresh(&mut self, ctx: &egui::Context) {
        if self.state.pending_cron_promise.is_some() { return; }
        self.state.cron_loading = true;
        let client = self.state.client.clone();
        let ctx2 = ctx.clone();
        let (sender, promise) = Promise::new();
        #[cfg(not(target_arch = "wasm32"))]
        spawn_task(&self.rt, async move {
            let result = client.list_cron_jobs().await.map_err(|e| e.to_string());
            sender.send(result);
            ctx2.request_repaint();
        });
        #[cfg(target_arch = "wasm32")]
        spawn_task(async move {
            let result = client.list_cron_jobs().await.map_err(|e| e.to_string());
            sender.send(result);
            ctx2.request_repaint();
        });
        self.state.pending_cron_promise = Some(promise);
    }

    fn poll_cron_promise(&mut self) {
        if let Some(ref p) = self.state.pending_cron_promise {
            if let Some(result) = p.ready() {
                self.state.cron_loading = false;
                match result {
                    Ok(jobs) => {
                        self.state.cron_jobs = jobs.clone();
                        self.state.cron_error = None;
                    }
                    Err(e) => {
                        self.state.cron_error = Some(e.clone());
                    }
                }
                self.state.pending_cron_promise = None;
            }
        }
    }

    fn do_snapshot_refresh(&mut self, ctx: &egui::Context) {
        if self.state.pending_snapshot_promise.is_some() { return; }
        let client = self.state.client.clone();
        let ctx2 = ctx.clone();
        let (sender, promise) = Promise::new();
        #[cfg(not(target_arch = "wasm32"))]
        spawn_task(&self.rt, async move {
            let result = client.get_snapshot().await.map_err(|e| e.to_string());
            sender.send(result);
            ctx2.request_repaint();
        });
        #[cfg(target_arch = "wasm32")]
        spawn_task(async move {
            let result = client.get_snapshot().await.map_err(|e| e.to_string());
            sender.send(result);
            ctx2.request_repaint();
        });
        self.state.pending_snapshot_promise = Some(promise);
    }

    fn poll_snapshot_promise(&mut self) {
        if let Some(ref p) = self.state.pending_snapshot_promise {
            if let Some(result) = p.ready() {
                match result {
                    Ok(snap) => { 
                        self.state.snapshot = Some(snap.clone());
                        // Auto-populate vault entries if missing — but skip deleted ones
                        for p in &snap.custom_providers {
                            let key_to_check = format!("{}_API_KEY", p.to_uppercase());
                            // Skip if user deleted this key in this session
                            if self.state.deleted_vault_keys.contains(&key_to_check) {
                                continue;
                            }
                            if !self.state.vault_entries.iter().any(|e| e.key.to_uppercase() == key_to_check) {
                                self.state.vault_entries.push(VaultEntry {
                                    key: key_to_check.clone(),
                                    saved: true,
                                    ..Default::default()
                                });
                            }
                        }
                        
                        // Mark existing entries as saved if the backend has them
                        let standard = ["OPENAI_API_KEY", "ANTHROPIC_API_KEY", "GEMINI_API_KEY", "DEEPSEEK_API_KEY", "MINIMAX_API_KEY"];
                        for entry in &mut self.state.vault_entries {
                            if snap.vault_keys.contains(&entry.key) {
                                entry.saved = true;
                            } else if !standard.contains(&entry.key.as_str()) {
                                // Custom entry NOT in backend vault: mark as unsaved
                                entry.saved = false;
                            }
                        }
                    }
                    Err(_) => {}
                }
                self.state.pending_snapshot_promise = None;
            }
        }
    }

    fn do_provider_refresh(&mut self, ctx: &egui::Context) {
        if self.state.pending_provider_promise.is_some() { return; }
        self.state.provider_loading = true;
        let client = self.state.client.clone();
        let ctx2 = ctx.clone();
        let (sender, promise) = Promise::new();
        #[cfg(not(target_arch = "wasm32"))]
        spawn_task(&self.rt, async move {
            let result = client.get_provider_schema().await.map_err(|e| e.to_string());
            sender.send(result);
            ctx2.request_repaint();
        });
        #[cfg(target_arch = "wasm32")]
        spawn_task(async move {
            let result = client.get_provider_schema().await.map_err(|e| e.to_string());
            sender.send(result);
            ctx2.request_repaint();
        });
        self.state.pending_provider_promise = Some(promise);
    }

    fn poll_provider_promise(&mut self) {
        if let Some(ref p) = self.state.pending_provider_promise {
            if let Some(result) = p.ready() {
                self.state.provider_loading = false;
                match result {
                    Ok(resp) => {
                        self.state.provider_metadata = resp.providers.clone();
                        self.state.provider_error = None;
                        
                        // Auto-populate vault entries with fields from metadata
                        for provider in &resp.providers {
                            for field in &provider.fields {
                                if !self.state.vault_entries.iter().any(|e| e.key == field.key) {
                                    self.state.vault_entries.push(VaultEntry {
                                        key: field.key.clone(),
                                        saved: false, // Will be checked by snapshot later
                                        ..Default::default()
                                    });
                                }
                            }
                        }
                    }
                    Err(e) => {
                        self.state.provider_error = Some(e.clone());
                    }
                }
                self.state.pending_provider_promise = None;
            }
        }
    }

    fn poll_install_promise(&mut self, ctx: &egui::Context) {
        if let Some(ref p) = self.state.pending_install_promise {
            if let Some(result) = p.ready() {
                self.state.store_installing = false;
                match result {
                    Ok(resp) => {
                        self.state.store_install_success =
                            Some(format!("Installed: {}", resp.skill_name));
                        self.state.store_install_url.clear();
                        // Immediately refresh the skills list
                        self.trigger_refresh(ctx);
                    }
                    Err(e) => {
                        self.state.store_install_error = Some(e.clone());
                    }
                }
                self.state.pending_install_promise = None;
            }
        }
    }

    /// Kick off an async Soul load for the currently-selected role.
    /// Safe to call multiple times; won't start a new request if one is already in-flight.
    fn trigger_load_soul(&mut self, ctx: &egui::Context) {
        self.state.persona_role_loaded = false; // Reset on new load request
        if self.state.persona_role_promise.is_some() { return; }
        let client = self.state.client.clone();
        let role = self.state.persona_role_selected.clone();
        let ctx2 = ctx.clone();
        let (sender, promise) = Promise::new();
        #[cfg(not(target_arch = "wasm32"))]
        spawn_task(&self.rt, async move {
            let res = client.get_soul(&role).await.map_err(|e| e.to_string());
            sender.send(res);
            ctx2.request_repaint();
        });
        #[cfg(target_arch = "wasm32")]
        spawn_task(async move {
            let res = client.get_soul(&role).await.map_err(|e| e.to_string());
            sender.send(res);
            ctx2.request_repaint();
        });
        self.state.persona_role_promise = Some(promise);
    }

    fn update_soul_fields_from_content(&mut self) {
        let content = &self.state.persona_role_content;
        if content.is_empty() { return; }

        // Find the frontmatter block between the first and second "---"
        let parts: Vec<&str> = content.split("---").collect();
        if parts.len() >= 3 {
            let yaml_str = parts[1];
            for line in yaml_str.lines() {
                let kv: Vec<&str> = line.splitn(2, ':').collect();
                if kv.len() == 2 {
                    let key = kv[0].trim().to_lowercase();
                    let val = kv[1].trim().trim_matches('"').trim_matches('\'').trim().to_string();
                    if !val.is_empty() {
                        match key.as_str() {
                            "provider" => self.state.persona_role_provider = val,
                            "base_url" => self.state.persona_role_base_url = val,
                            "model" => self.state.persona_role_model = val,
                            "temperature" => self.state.persona_role_temperature = val,
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    fn update_soul_content_from_fields(&mut self) {
        let content = self.state.persona_role_content.clone();
        
        // If content is completely empty, don't force a frontmatter block back in.
        // This allows users to completely delete settings if they wish.
        if content.trim().is_empty() {
             return;
        }

        let mut fm = String::from("---\n");
        fm.push_str(&format!("provider: {}\n", self.state.persona_role_provider.trim()));
        if !self.state.persona_role_base_url.is_empty() {
             fm.push_str(&format!("base_url: {}\n", self.state.persona_role_base_url.trim()));
        }
        fm.push_str(&format!("model: {}\n", self.state.persona_role_model.trim()));
        fm.push_str(&format!("temperature: {}\n", self.state.persona_role_temperature.trim()));
        fm.push_str("---\n\n");
        let frontmatter = fm;

        // More aggressive replacement: look for the first TWO occurrences of ---
        let mut _first_dash = None;
        let mut second_dash = None;
        
        if let Some(first) = content.find("---") {
            _first_dash = Some(first);
            if let Some(second) = content[first + 3..].find("---") {
                second_dash = Some(first + 3 + second);
            }
        }
        
        let body = if let Some(s) = second_dash {
            &content[s + 3..]
        } else {
            content.as_str()
        };
        
        self.state.persona_role_content = format!("{}{}", frontmatter, body.trim_start());
    }

    fn do_persona_refresh(&mut self, ctx: &egui::Context) {
        let client = self.state.client.clone();
        let ctx2 = ctx.clone();
        let (sender, promise) = Promise::new();
        #[cfg(not(target_arch = "wasm32"))]
        spawn_task(&self.rt, async move {
            let res = client.list_souls().await.map_err(|e| e.to_string());
            sender.send(res);
            ctx2.request_repaint();
        });
        #[cfg(target_arch = "wasm32")]
        spawn_task(async move {
            let res = client.list_souls().await.map_err(|e| e.to_string());
            sender.send(res);
            ctx2.request_repaint();
        });
        self.state.persona_souls_promise = Some(promise);
    }

    fn do_persona_templates_refresh(&mut self, ctx: &egui::Context) {
        let client = self.state.client.clone();
        let ctx2 = ctx.clone();
        let (sender, promise) = Promise::new();
        #[cfg(not(target_arch = "wasm32"))]
        spawn_task(&self.rt, async move {
            let res = client.get_persona_templates().await.map_err(|e| e.to_string());
            sender.send(res);
            ctx2.request_repaint();
        });
        #[cfg(target_arch = "wasm32")]
        spawn_task(async move {
            let res = client.get_persona_templates().await.map_err(|e| e.to_string());
            sender.send(res);
            ctx2.request_repaint();
        });
        self.state.persona_templates_promise = Some(promise);
    }
    fn show_persona_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                if ui.selectable_label(self.state.persona_subtab == PersonaSubTab::Editor, t("tabs.persona", self.state.language)).clicked() {
                    self.state.persona_subtab = PersonaSubTab::Editor;
                }
                if ui.selectable_label(self.state.persona_subtab == PersonaSubTab::Gallery, t("blueprint.gallery", self.state.language)).clicked() {
                    self.state.persona_subtab = PersonaSubTab::Gallery;
                }
            });
            ui.separator();
            ui.add_space(8.0);

            match self.state.persona_subtab {
                PersonaSubTab::Editor => self.show_persona_editor(ui, ctx),
                PersonaSubTab::Gallery => self.show_persona_gallery(ui, ctx),
            }
        });
    }

    fn show_persona_editor(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if !self.state.persona_role_loaded && self.state.persona_role_promise.is_none() {
            self.trigger_load_soul(ctx);
        }
        if self.state.persona_souls_promise.is_none() && self.state.persona_souls.is_empty() {
            self.do_persona_refresh(ctx);
        }
        if !self.state.persona_heartbeat_loaded && self.state.persona_heartbeat_promise.is_none() {
            let client = self.state.client.clone();
            let ctx2 = ctx.clone();
            let (sender, promise) = Promise::new();
            #[cfg(not(target_arch = "wasm32"))]
            spawn_task(&self.rt, async move {
                let res = client.get_heartbeat().await.map_err(|e| e.to_string());
                sender.send(res);
                ctx2.request_repaint();
            });
            #[cfg(target_arch = "wasm32")]
            spawn_task(async move {
                let res = client.get_heartbeat().await.map_err(|e| e.to_string());
                sender.send(res);
                ctx2.request_repaint();
            });
            self.state.persona_heartbeat_promise = Some(promise);
        }

        ui.heading(RichText::new("Soul & System Prompt Editor").color(palette::text_bright(self.state.night_mode)));
        ui.add_space(8.0);
        ui.label(RichText::new("Configure the Global System Prompt (HEARTBEAT) and individual Agent Souls.").color(palette::text_dim(self.state.night_mode)).small());
        ui.add_space(8.0);
        
        let screen_height = ctx.screen_rect().height();
        let desired_height = ((screen_height - 300.0) / 2.0).max(200.0);
        
        ui.horizontal(|ui| {
            ui.label(RichText::new("Global System Prompt (HEARTBEAT.md):").color(palette::ACCENT));
            
            ui.menu_button(RichText::new("Templates").small(), |ui| {
                ui.label(RichText::new("System Architectures").strong().color(palette::ACCENT));
                if ui.button("AIMAXXING Gateway Default").clicked() { self.apply_heartbeat_template("AIMAXXING Default"); ui.close_menu(); }
                if ui.button("High-Density Minimal").clicked() { self.apply_heartbeat_template("High-Density Minimal"); ui.close_menu(); }
                if ui.button("Pure Autonomous Coder").clicked() { self.apply_heartbeat_template("Auto Coder"); ui.close_menu(); }
            });

            ui.add_space(8.0);

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("💾 Save Heartbeat").clicked() {
                    let client = self.state.client.clone();
                    let content = self.state.persona_heartbeat_content.clone();
                    let ctx2 = ctx.clone();
                    let (sender, promise) = Promise::new();
                    #[cfg(not(target_arch = "wasm32"))]
                    spawn_task(&self.rt, async move {
                        let res = client.put_heartbeat(content).await.map_err(|e| e.to_string());
                        sender.send(res);
                        ctx2.request_repaint();
                    });
                    #[cfg(target_arch = "wasm32")]
                    spawn_task(async move {
                        let res = client.put_heartbeat(content).await.map_err(|e| e.to_string());
                        sender.send(res);
                        ctx2.request_repaint();
                    });
                    self.state.persona_save_promise = Some(promise);
                    self.state.persona_heartbeat_dirty = false;
                }

                if ui.small_button("🔄 Revert").on_hover_text("Revert").clicked() {
                    let client = self.state.client.clone();
                    let ctx2 = ctx.clone();
                    let (sender, promise) = Promise::new();
                    #[cfg(not(target_arch = "wasm32"))]
                    spawn_task(&self.rt, async move {
                        let res = client.get_heartbeat().await.map_err(|e| e.to_string());
                        sender.send(res);
                        ctx2.request_repaint();
                    });
                    #[cfg(target_arch = "wasm32")]
                    spawn_task(async move {
                        let res = client.get_heartbeat().await.map_err(|e| e.to_string());
                        sender.send(res);
                        ctx2.request_repaint();
                    });
                    self.state.persona_heartbeat_promise = Some(promise);
                }
            });
        });
        let mut heartbeat = self.state.persona_heartbeat_content.clone();
        
        egui::Frame::new()
            .fill(self.theme_bg_deep())
            .stroke(Stroke::new(1.0, palette::border(self.state.night_mode)))
            .corner_radius(egui::CornerRadius::same(6))
            .inner_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                egui::ScrollArea::vertical().id_salt("heartbeat_scroll").min_scrolled_height(desired_height).max_height(desired_height).show(ui, |ui| {
                    let response = ui.add_sized(
                        [ui.available_width(), desired_height],
                        egui::TextEdit::multiline(&mut heartbeat)
                            .font(egui::TextStyle::Monospace)
                            .code_editor()
                            .frame(false)
                    );
                    if response.changed() {
                        self.state.persona_heartbeat_content = heartbeat;
                        self.state.persona_heartbeat_dirty = true;
                    }
                });
            });

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);

        // Row 1: Persona and Gallery
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            
            // Persona Selection
            ui.label(RichText::new("Soul:").strong().color(palette::ACCENT));
            ui.add_space(12.0);
            
            if self.state.persona_souls.is_empty() && self.state.persona_souls_promise.is_none() {
                self.do_persona_refresh(ctx);
            }

            let mut known_roles = std::collections::BTreeSet::new();
            for soul in &self.state.persona_souls { known_roles.insert(soul.clone()); }
            for custom in &self.state.custom_added_personas { known_roles.insert(custom.clone()); }

            egui::ComboBox::from_id_salt("persona_select_ovr_v2")
                .selected_text(&self.state.persona_role_selected)
                .show_ui(ui, |ui| {
                    for role in &known_roles {
                        if role.trim().is_empty() { continue; }
                        if ui.selectable_value(&mut self.state.persona_role_selected, role.clone(), role).clicked() {
                            self.state.persona_role_content.clear();
                            self.trigger_load_soul(ctx);
                        }
                    }
                });

            ui.add_space(12.0); 
            let name_w = (self.state.persona_role_selected.len() as f32 * 7.5 + 12.0).max(60.0).min(220.0);
            let name_resp = egui::Frame::new()
                .fill(Color32::from_rgb(28, 28, 32))
                .stroke(Stroke::new(1.0, palette::border(self.state.night_mode)))
                .corner_radius(egui::CornerRadius::same(4))
                .inner_margin(egui::Margin::symmetric(6, 3))
                .show(ui, |ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.state.persona_role_selected)
                        .desired_width(name_w)
                        .hint_text("Name...")
                        .frame(false))
                }).inner;
            
            if (name_resp.lost_focus() || (name_resp.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))) 
                && !self.state.persona_role_selected.is_empty() 
                && !known_roles.contains(&self.state.persona_role_selected) 
            {
                self.state.custom_added_personas.insert(self.state.persona_role_selected.clone());
                self.state.persona_role_content.clear();
                self.trigger_load_soul(ctx);
            }

            // Gallery (API-driven, Phase 11-A)
            ui.label(RichText::new("Gallery:").strong().color(palette::ACCENT));
            ui.add_space(8.0);

            // Lazy-load blueprints from API on first open
            if self.state.blueprints.is_empty() && self.state.blueprints_promise.is_none() {
                let client = self.state.client.clone();
                let (sender, promise) = Promise::new();
                #[cfg(not(target_arch = "wasm32"))]
                spawn_task(&self.rt, async move {
                    let res = client.list_blueprints().await.map_err(|e| e.to_string());
                    sender.send(res);
                });
                #[cfg(target_arch = "wasm32")]
                spawn_task(async move {
                    let res = client.list_blueprints().await.map_err(|e| e.to_string());
                    sender.send(res);
                });
                self.state.blueprints_promise = Some(promise);
            }
            // Poll promise
            if let Some(promise) = self.state.blueprints_promise.take() {
                match promise.try_take() {
                    Ok(Ok(bps)) => { self.state.blueprints = bps; }
                    Ok(Err(e)) => { self.state.set_status(format!("Blueprint load error: {}", e), true); }
                    Err(promise) => { self.state.blueprints_promise = Some(promise); }
                }
            }
            // Poll apply promise
            if let Some(promise) = self.state.blueprint_apply_promise.take() {
                match promise.try_take() {
                    Ok(Ok(())) => {
                        self.state.set_status("Blueprint applied successfully!", false);
                        self.state.persona_role_content.clear();
                        self.state.persona_role_loaded = false;
                        self.trigger_load_soul(ctx);
                    }
                    Ok(Err(e)) => { self.state.set_status(format!("Blueprint apply error: {}", e), true); }
                    Err(promise) => { self.state.blueprint_apply_promise = Some(promise); }
                }
            }

            ui.menu_button(RichText::new("⚡ Templates").small(), |ui| {
                // Custom blank option
                if ui.button("Custom (Blank)").clicked() {
                    self.apply_blueprint_template("Custom");
                    ui.close_menu();
                }
                ui.separator();

                // Group by category from the API
                let blueprints = self.state.blueprints.clone();
                let mut last_category = String::new();
                for bp in &blueprints {
                    if bp.category != last_category {
                        if !last_category.is_empty() { ui.separator(); }
                        ui.label(RichText::new(&bp.category).strong().color(palette::ACCENT));
                        last_category = bp.category.clone();
                    }
                    let btn_label = format!("{}", bp.name);
                    if ui.button(&btn_label).on_hover_text(&bp.description).clicked() {
                        // Apply via API: write SOUL.md + hot-reload agent
                        let client = self.state.client.clone();
                        let bp_id = bp.id.clone();
                        let role = self.state.persona_role_selected.clone();
                        self.state.blueprint_apply_role = role.clone();
                        let (sender, promise) = Promise::new();
                        #[cfg(not(target_arch = "wasm32"))]
                        spawn_task(&self.rt, async move {
                            let res = client.apply_blueprint(&bp_id, &role).await.map_err(|e| e.to_string());
                            sender.send(res);
                        });
                        #[cfg(target_arch = "wasm32")]
                        spawn_task(async move {
                            let res = client.apply_blueprint(&bp_id, &role).await.map_err(|e| e.to_string());
                            sender.send(res);
                        });
                        self.state.blueprint_apply_promise = Some(promise);
                        ui.close_menu();
                    }
                }

                if blueprints.is_empty() {
                    ui.label(RichText::new("Loading...").weak());
                }
            });
        });

        ui.add_space(8.0);

        // Row 2: Overrides and Actions (Vercel-style dynamic metadata)
        ui.horizontal(|ui| {
            let mut changed = false;
            
            ui.label(RichText::new("Provider:").strong().color(palette::ACCENT)); 
            ui.add_space(8.0);
            
            // Dynamic provider metadata from backend
            let providers = self.state.provider_metadata.clone();
            
            // Current provider metadata (if exists)
            let selected_p_meta = providers.iter().find(|p| p.id == self.state.persona_role_provider.to_lowercase());

            egui::ComboBox::from_id_salt("soul_provider_v4_dynamic")
                .selected_text(selected_p_meta.map(|p| p.name.clone()).unwrap_or(self.state.persona_role_provider.clone()))
                .show_ui(ui, |ui| {
                    if providers.is_empty() {
                        ui.label(RichText::new("Loading providers...").weak().small());
                    }
                    for p in &providers {
                        if ui.selectable_value(&mut self.state.persona_role_provider, p.id.clone(), &p.name).clicked() {
                            changed = true;
                            // Set first preferred model as default if current model is empty/placeholder
                            if self.state.persona_role_model.is_empty() || self.state.persona_role_model == "gpt-4o" {
                                if let Some(m) = p.preferred_models.first() {
                                    self.state.persona_role_model = m.clone();
                                }
                            }
                        }
                    }
                });
            
            ui.add_space(12.0); 
            // Inline editable provider for "custom" override
            let p_w = (self.state.persona_role_provider.len() as f32 * 7.5 + 12.0).max(60.0);
            let p_resp = egui::Frame::new()
                .fill(Color32::from_rgb(28, 28, 32))
                .stroke(Stroke::new(1.0, palette::border(self.state.night_mode)))
                .corner_radius(egui::CornerRadius::same(4))
                .inner_margin(egui::Margin::symmetric(6, 3))
                .show(ui, |ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.state.persona_role_provider)
                        .desired_width(p_w)
                        .frame(false))
                }).inner;
            if p_resp.changed() { changed = true; }

            ui.add_space(24.0);
            ui.label(RichText::new("Model:").strong().color(palette::ACCENT)); 
            ui.add_space(8.0);

            // Prefer models from provider metadata if available
            if let Some(p) = selected_p_meta {
                egui::ComboBox::from_id_salt("soul_model_v4_dynamic")
                    .selected_text(&self.state.persona_role_model)
                    .width(160.0)
                    .show_ui(ui, |ui| {
                        for m in &p.preferred_models {
                            changed |= ui.selectable_value(&mut self.state.persona_role_model, m.clone(), m).changed();
                        }
                    });
                ui.add_space(8.0);
            }

            let m_w = (self.state.persona_role_model.len() as f32 * 7.5 + 12.0).max(80.0);
            let m_resp = egui::Frame::new()
                .fill(Color32::from_rgb(28, 28, 32))
                .stroke(Stroke::new(1.0, palette::border(self.state.night_mode)))
                .corner_radius(egui::CornerRadius::same(4))
                .inner_margin(egui::Margin::symmetric(6, 3))
                .show(ui, |ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.state.persona_role_model)
                        .desired_width(m_w)
                        .frame(false))
                }).inner;
            if m_resp.changed() { changed = true; }

            ui.add_space(24.0);
            ui.label(RichText::new("Temp:").strong().color(palette::ACCENT)); 
            ui.add_space(8.0);
            let t_w = (self.state.persona_role_temperature.len() as f32 * 7.5 + 12.0).max(40.0);
            let t_resp = egui::Frame::new()
                .fill(Color32::from_rgb(28, 28, 32))
                .stroke(Stroke::new(1.0, palette::border(self.state.night_mode)))
                .corner_radius(egui::CornerRadius::same(4))
                .inner_margin(egui::Margin::symmetric(6, 3))
                .show(ui, |ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.state.persona_role_temperature)
                        .desired_width(t_w)
                        .frame(false))
                }).inner;
            if t_resp.changed() { changed = true; }

            if changed {
                self.update_soul_content_from_fields();
                self.state.persona_role_dirty = true;
            }
        });
        
        ui.add_space(12.0);

        // Row 3: Action Buttons
        ui.horizontal(|ui| {
            ui.label(RichText::new(t("soul.memory_depth", self.state.language)).small());
            ui.add(egui::Slider::new(&mut self.state.persona_export_limit, 0..=500));
            
            if self.state.persona_export_promise.is_some() {
                ui.spinner();
            } else {
                if ui.button(RichText::new(format!("📤 {}", t("soul.export", self.state.language))).color(palette::ACCENT)).clicked() {
                    let client = self.state.client.clone();
                    let role = self.state.persona_role_selected.clone();
                    let limit = self.state.persona_export_limit;
                    let ctx2 = ctx.clone();
                    let (sender, promise) = Promise::new();
                    #[cfg(not(target_arch = "wasm32"))]
                    spawn_task(&self.rt, async move {
                        let res = client.export_soul(&role, limit).await.map_err(|e| e.to_string());
                        sender.send(res);
                        ctx2.request_repaint();
                    });
                    #[cfg(target_arch = "wasm32")]
                    spawn_task(async move {
                        let res = client.export_soul(&role, limit).await.map_err(|e| e.to_string());
                        sender.send(res);
                        ctx2.request_repaint();
                    });
                    self.state.persona_export_promise = Some(promise);
                }
            }

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(12.0);

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("💾 Save Soul").clicked() {
                    self.update_soul_content_from_fields();
                    let client = self.state.client.clone();
                    let role = self.state.persona_role_selected.clone();
                    let content = self.state.persona_role_content.clone();
                    let ctx2 = ctx.clone();
                    let (sender, promise) = Promise::new();
                    #[cfg(not(target_arch = "wasm32"))]
                    spawn_task(&self.rt, async move {
                        let res = client.put_soul(&role, content).await.map_err(|e| e.to_string());
                        sender.send(res);
                        ctx2.request_repaint();
                    });
                    #[cfg(target_arch = "wasm32")]
                    spawn_task(async move {
                        let res = client.put_soul(&role, content).await.map_err(|e| e.to_string());
                        sender.send(res);
                        ctx2.request_repaint();
                    });
                    self.state.persona_save_promise = Some(promise);
                    self.state.persona_role_dirty = false;
                }

                if ui.small_button("🔄 Revert").clicked() {
                    self.state.persona_role_content.clear();
                    self.trigger_load_soul(ctx);
                }

                let is_core = self.state.persona_templates.iter().any(|t| t.name == self.state.persona_role_selected) 
                    || self.state.persona_role_selected == "assistant"; // assistant is always core/fallback
                if !is_core && !self.state.persona_role_selected.is_empty() {
                    if ui.button(RichText::new("🗑 Delete").color(palette::DANGER)).clicked() {
                        let role_to_delete = self.state.persona_role_selected.clone();
                        self.state.custom_added_personas.remove(&role_to_delete);
                        self.state.persona_souls.retain(|r| r != &role_to_delete);
                        let client = self.state.client.clone();
                        #[cfg(not(target_arch = "wasm32"))]
                        spawn_task(&self.rt, async move { let _ = client.delete_soul(&role_to_delete).await; });
                        #[cfg(target_arch = "wasm32")]
                        spawn_task(async move { let _ = client.delete_soul(&role_to_delete).await; });
                        self.state.persona_role_selected = "assistant".to_string();
                        self.state.persona_role_content.clear();
                        self.trigger_load_soul(ctx);
                    }
                }
            });
        });

        ui.add_space(8.0);
        
        ui.label(RichText::new(format!("{} Soul (SOUL.md):", self.state.persona_role_selected.to_uppercase())).color(palette::ACCENT));
        let mut role_content = self.state.persona_role_content.clone();
        
        egui::Frame::new()
            .fill(self.theme_bg_deep())
            .stroke(Stroke::new(1.0, palette::border(self.state.night_mode)))
            .corner_radius(egui::CornerRadius::same(6))
            .inner_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                egui::ScrollArea::vertical().id_salt("role_scroll").min_scrolled_height(desired_height).max_height(desired_height).show(ui, |ui| {
                    let response = ui.add_sized(
                        [ui.available_width(), desired_height],
                        egui::TextEdit::multiline(&mut role_content)
                            .font(egui::TextStyle::Monospace)
                            .code_editor()
                            .lock_focus(true)
                            .frame(false)
                    );
                    if response.changed() {
                        self.state.persona_role_content = role_content;
                        self.state.persona_role_dirty = true;
                        self.update_soul_fields_from_content();
                    }
                });
            });
    }

    fn poll_persona_promises(&mut self, ctx: &egui::Context) {
        let mut heartbeat_res = None;
        if let Some(ref mut p) = self.state.persona_heartbeat_promise {
            if let Some(res) = p.ready_mut() { heartbeat_res = Some(res.clone()); }
        }
        if let Some(res) = heartbeat_res {
            self.state.persona_heartbeat_promise = None;
            match res {
                Ok(cnt) => { 
                    self.state.persona_heartbeat_content = cnt; 
                    self.state.persona_heartbeat_loaded = true;
                }
                Err(e) => { self.state.status_msg = Some((format!("Failed to load heartbeat: {}", e), true)); }
            }
        }

        let mut souls_res = None;
        if let Some(ref mut p) = self.state.persona_souls_promise {
            if let Some(res) = p.ready_mut() { souls_res = Some(res.clone()); }
        }
        if let Some(res) = souls_res {
            self.state.persona_souls_promise = None;
            match res {
                Ok(souls_ref) => {
                    let mut souls = souls_ref.clone();
                    // Inject names from templates if they are missing
                    for t in &self.state.persona_templates {
                        if !souls.contains(&t.name) {
                            souls.push(t.name.clone());
                        }
                    }
                    // Compatibility: assistant, researcher, evo are the hardcoded minimums if no templates yet
                    if self.state.persona_templates.is_empty() {
                         let core = ["assistant", "researcher", "evo"];
                         for c in core {
                             if !souls.contains(&c.to_string()) {
                                 souls.push(c.to_string());
                             }
                         }
                    }
                    souls.sort();
                    self.state.persona_souls = souls;
                }
                Err(e) => { self.state.status_msg = Some((format!("Failed to list souls: {}", e), true)); }
            }
        }

        let mut templates_res = None;
        if let Some(ref mut p) = self.state.persona_templates_promise {
            if let Some(res) = p.ready_mut() { templates_res = Some(res.clone()); }
        }
        if let Some(res) = templates_res {
            self.state.persona_templates_promise = None;
            match res {
                Ok(templates) => {
                    self.state.persona_templates = templates;
                    // Trigger a souls refresh to merge names
                    self.do_persona_refresh(ctx);
                }
                Err(e) => { self.state.status_msg = Some((format!("Failed to load persona templates: {}", e), true)); }
            }
        }

        let mut role_res = None;
        if let Some(ref mut p) = self.state.persona_role_promise {
            if let Some(res) = p.ready_mut() { role_res = Some(res.clone()); }
        }
        if let Some(res) = role_res {
            self.state.persona_role_promise = None;
            match res {
                Ok(cnt) => {
                    self.state.persona_role_content = cnt;
                    self.state.persona_role_loaded = true;
                    self.update_soul_fields_from_content();
                }
                Err(e) => {
                    self.state.status_msg = Some((format!("Failed to load soul: {}", e), true));
                }
            }
        }

        let mut save_res = None;
        if let Some(ref mut p) = self.state.persona_save_promise {
            if let Some(res) = p.ready_mut() { save_res = Some(res.clone()); }
        }
        if let Some(res) = save_res {
            self.state.persona_save_promise = None;
            match res {
                Ok(_) => { 
                    self.state.status_msg = Some(("Saved successfully.".into(), false)); 
                    self.do_persona_refresh(ctx);
                }
                Err(e) => { self.state.status_msg = Some((format!("Failed to save: {}", e), true)); }
            }
        }
    }

    fn poll_persona_export_promise(&mut self, _ctx: &egui::Context) {
        let mut export_res = None;
        if let Some(ref mut p) = self.state.persona_export_promise {
            if let Some(res) = p.ready_mut() { export_res = Some(res.clone()); }
        }
        if let Some(res) = export_res {
            self.state.persona_export_promise = None;
            match res {
                Ok(json) => {
                    self.state.persona_export_json = Some(json);
                    self.state.set_status(t("soul.export_success", self.state.language), false);
                }
                Err(e) => {
                    self.state.set_status(format!("{}: {}", t("soul.export_failed", self.state.language), e), true);
                }
            }
        }
    }

    fn show_export_result_window(&mut self, ctx: &egui::Context) {
        let mut open = true;
        egui::Window::new(t("soul.export", self.state.language))
            .open(&mut open)
            .resizable(true)
            .default_width(600.0)
            .show(ctx, |ui| {
                ui.label(RichText::new("Copy the JSON below to save your Agent Vessel:").small());
                ui.add_space(8.0);
                if let Some(mut json) = self.state.persona_export_json.clone() {
                    egui::ScrollArea::vertical().max_height(400.0).show(ui, |ui| {
                        ui.add(egui::TextEdit::multiline(&mut json)
                            .font(egui::TextStyle::Monospace)
                            .desired_width(f32::INFINITY));
                    });
                    ui.add_space(8.0);
                    if ui.button("📋 Copy to Clipboard").clicked() {
                        ui.ctx().copy_text(json);
                    }
                }
            });
        if !open {
            self.state.persona_export_json = None;
        }
    }

    fn show_chat_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.vertical(|ui| {
            ui.heading(RichText::new("Chat with Agents").color(palette::text_bright(self.state.night_mode)));
            ui.add_space(8.0);

            // Agent Selection
            ui.horizontal(|ui| {
                ui.label("Talk to:");
                egui::ComboBox::from_id_salt("chat_role_select")
                    .selected_text(&self.state.chat_selected_role)
                    .show_ui(ui, |ui| {
                        for role in &self.state.persona_souls {
                            ui.selectable_value(&mut self.state.chat_selected_role, role.clone(), role);
                        }
                    });
                
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                   if ui.button("🗑 Clear").clicked() {
                       self.state.chat_histories.remove(&self.state.chat_selected_role);
                   }
                });
            });
            ui.add_space(8.0);

            // Use bottom_up to pin input to the bottom
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                ui.add_space(8.0);
                
                // Input Area at the very bottom
                ui.horizontal(|ui| {
                    let text_edit = egui::TextEdit::singleline(&mut self.state.chat_input)
                        .hint_text("Type a message...")
                        .desired_width(ui.available_width() - 160.0);
                    
                    let response = ui.add(text_edit);
                    ui.add_space(4.0);
                    
                    if (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) || ui.button("  Send  ").clicked() {
                        self.do_chat_send(ctx);
                        response.request_focus();
                    }

                    // Phase 11-B: Red STOP button for task cancellation
                    if self.state.chat_loading {
                        let stop_btn = egui::Button::new(
                            RichText::new("🛑 STOP").color(Color32::WHITE).strong()
                        ).fill(Color32::from_rgb(200, 40, 40));

                        if ui.add(stop_btn).clicked() && self.state.cancel_promise.is_none() {
                            let client = self.state.client.clone();
                            let (sender, promise) = Promise::new();
                            #[cfg(not(target_arch = "wasm32"))]
                            spawn_task(&self.rt, async move {
                                let res = client.cancel_task().await.map_err(|e| e.to_string());
                                sender.send(res);
                            });
                            #[cfg(target_arch = "wasm32")]
                            spawn_task(async move {
                                let res = client.cancel_task().await.map_err(|e| e.to_string());
                                sender.send(res);
                            });
                            self.state.cancel_promise = Some(promise);
                        }
                    }

                    // Poll cancel promise
                    if let Some(promise) = self.state.cancel_promise.take() {
                        match promise.try_take() {
                            Ok(Ok(())) => {
                                self.state.set_status("Task cancelled.", false);
                                self.state.chat_loading = false;
                            }
                            Ok(Err(e)) => { self.state.set_status(format!("Cancel error: {}", e), true); }
                            Err(promise) => { self.state.cancel_promise = Some(promise); }
                        }
                    }
                });

                ui.add_space(8.0);

                egui::ScrollArea::vertical()
                    .id_salt("chat_history")
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        // Avoid cloning the entire history vector every frame
                        let history_ref = self.state.chat_histories.get(&self.state.chat_selected_role);
                        
                        // Force top-down layout inside the scroll area
                        ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                            if let Some(history) = history_ref {
                                for msg in history {
                                    let is_user = msg.role == "user";
                                    let align = if is_user { egui::Align::RIGHT } else { egui::Align::LEFT };
                                    
                                    ui.with_layout(egui::Layout::top_down(align), |ui| {
                                        let bg = if is_user { Color32::from_rgb(40, 60, 80) } else { self.theme_bg_deep() };
                                        let label_color = if is_user { palette::ACCENT } else { palette::text_bright(self.state.night_mode) };
                                        
                                        egui::Frame::new()
                                            .fill(bg)
                                            .stroke(Stroke::new(1.0, palette::border(self.state.night_mode)))
                                            .corner_radius(egui::CornerRadius::same(8))
                                            .inner_margin(egui::Margin::symmetric(14, 10))
                                            .show(ui, |ui| {
                                                if let Some(name) = &msg.agent_name {
                                                    ui.label(RichText::new(name).small().color(palette::text_dim(self.state.night_mode)));
                                                }
                                                ui.label(RichText::new(&msg.content).color(label_color));
                                            });
                                    });
                                    ui.add_space(6.0);
                                }
                            }
                        });
                    });
            });
        });
    }

    fn do_chat_send(&mut self, ctx: &egui::Context) {
        let text = self.state.chat_input.trim().to_string();
        if text.is_empty() || self.state.chat_loading { return; }

        // Add user message locally
        let history = self.state.chat_histories.entry(self.state.chat_selected_role.clone()).or_default();
        history.push(crate::app_state::ChatMessage {
            role: "user".to_string(),
            content: text.clone(),
            agent_name: None,
        });
        self.state.chat_input.clear();
        self.state.chat_loading = true;

        let client = self.state.client.clone();
        let role = Some(self.state.chat_selected_role.clone());
        let ctx2 = ctx.clone();
        let (sender, promise) = Promise::new();

        #[cfg(not(target_arch = "wasm32"))]
        spawn_task(&self.rt, async move {
            let res = client.chat(text, role, None).await.map_err(|e| e.to_string());
            sender.send(res);
            ctx2.request_repaint();
        });
        #[cfg(target_arch = "wasm32")]
        spawn_task(async move {
            let res = client.chat(text, role, None).await.map_err(|e| e.to_string());
            sender.send(res);
            ctx2.request_repaint();
        });

        self.state.chat_promise = Some(promise);
    }

    fn poll_chat_promise(&mut self, _ctx: &egui::Context) {
        if let Some(ref mut p) = self.state.chat_promise {
            if let Some(result) = p.ready_mut() {
                self.state.chat_loading = false;
                match result {
                    Ok(resp) => {
                        let history = self.state.chat_histories.entry(self.state.chat_selected_role.clone()).or_default();
                        history.push(crate::app_state::ChatMessage {
                            role: "agent".to_string(),
                            content: resp.clone(),
                            agent_name: Some(self.state.chat_selected_role.clone()),
                        });
                    }
                    Err(e) => {
                        self.state.status_msg = Some((format!("Chat error: {}", e), true));
                    }
                }
                self.state.chat_promise = None;
            }
        }
    }
    fn show_persona_gallery(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let lang = self.state.language;
        let night = self.state.night_mode;

        if self.state.blueprints_promise.is_none() && self.state.blueprints.is_empty() {
            let client = self.state.client.clone();
            let ctx2 = ctx.clone();
            let (sender, promise) = Promise::new();
            #[cfg(not(target_arch = "wasm32"))]
            spawn_task(&self.rt, async move {
                let res = client.list_blueprints().await.map_err(|e| e.to_string());
                sender.send(res);
                ctx2.request_repaint();
            });
            #[cfg(target_arch = "wasm32")]
            spawn_task(async move {
                let res = client.list_blueprints().await.map_err(|e| e.to_string());
                sender.send(res);
                ctx2.request_repaint();
            });
            self.state.blueprints_promise = Some(promise);
        }

        if let Some(p) = &self.state.blueprints_promise {
            let res: Option<&Result<Vec<BlueprintInfo>, String>> = p.ready();
            if let Some(res) = res {
                match res {
                    Ok(data) => self.state.blueprints = data.clone(),
                    _ => {}
                }
                self.state.blueprints_promise = None;
            }
        }

        ui.heading(RichText::new(t("blueprint.gallery", lang)).color(palette::text_bright(night)));
        ui.add_space(8.0);

        if self.state.blueprints.is_empty() {
            if self.state.blueprints_promise.is_some() {
                ui.vertical_centered(|ui| {
                    ui.add_space(40.0);
                    ui.spinner();
                    ui.label(t("misc.searching", lang));
                });
            } else {
                ui.label("No blueprints found.");
            }
            return;
        }

        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.add_space(4.0);
            let blueprints = self.state.blueprints.clone();
            for bp in blueprints {
                egui::Frame::new()
                    .fill(palette::bg_surface(night))
                    .stroke(Stroke::new(1.0, palette::border(night)))
                    .corner_radius(egui::CornerRadius::same(6))
                    .inner_margin(egui::Margin::same(10))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(&bp.name).strong().color(palette::text_bright(night)));
                                    ui.add_space(8.0);
                                    ui.label(RichText::new(&bp.category).small().color(palette::ACCENT));
                                });
                                ui.label(RichText::new(&bp.description).small().color(palette::text_dim(night)));
                            });

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.button(t("blueprint.apply", lang)).clicked() {
                                    self.state.blueprint_apply_role = bp.id.clone();
                                    // Normally we'd show a modal here to ask for a role name
                                    // For now, let's just apply it to the selected role
                                    let client = self.state.client.clone();
                                    let bp_id = bp.id.clone();
                                    let role = self.state.persona_role_selected.clone();
                                    let ctx2 = ctx.clone();
                                    let (sender, promise) = Promise::new();
                                    #[cfg(not(target_arch = "wasm32"))]
                                    spawn_task(&self.rt, async move {
                                        let res = client.apply_blueprint(&bp_id, &role).await.map_err(|e| e.to_string());
                                        sender.send(res);
                                        ctx2.request_repaint();
                                    });
                                    #[cfg(target_arch = "wasm32")]
                                    spawn_task(async move {
                                        let res = client.apply_blueprint(&bp_id, &role).await.map_err(|e| e.to_string());
                                        sender.send(res);
                                        ctx2.request_repaint();
                                    });
                                    self.state.blueprint_apply_promise = Some(promise);
                                }
                            });
                        });
                    });
                ui.add_space(8.0);
            }
        });
    }

    // ── System Tab ────────────────────────────────────────────────────────────
    fn show_system_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let lang = self.state.language;
        let night = self.state.night_mode;
        ui.vertical(|ui| {
            ui.heading(t("system.diagnostics", lang));
            ui.add_space(8.0);

            if ui.button(format!("🚀 {}", t("system.run_doctor", lang))).clicked() {
                self.do_doctor_run(ctx);
            }

            ui.add_space(16.0);

            if self.state.doctor_loading {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(t("misc.searching", lang));
                });
            } else if let Some(err) = &self.state.doctor_error {
                ui.label(RichText::new(format!("Error: {}", err)).color(palette::DANGER));
            } else if let Some(results) = &self.state.doctor_results {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for res in results {
                        egui::Frame::new()
                            .fill(self.theme_bg_deep())
                            .stroke(Stroke::new(1.0, palette::border(self.state.night_mode)))
                            .corner_radius(egui::CornerRadius::same(6))
                            .inner_margin(egui::Margin::same(10))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    let icon = if res.success { "✅" } else { "❌" };
                                    let color = if res.success { palette::SUCCESS } else { palette::DANGER };
                                    ui.label(RichText::new(icon).color(color).strong());
                                    ui.vertical(|ui| {
                                        ui.label(RichText::new(&res.name).strong());
                                        ui.label(RichText::new(&res.message).small().color(palette::text_dim(night)));
                                    });
                                });
                            });
                        ui.add_space(4.0);
                    }
                });
            } else {
                ui.label("Click the button above to start diagnostics.");
            }

            ui.separator();
            ui.add_space(16.0);
            
            ui.horizontal(|ui| {
                ui.heading("Sandbox Management");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("↻ Refresh").clicked() {
                        self.do_sandboxes_refresh(ctx);
                    }
                    if self.state.sandboxes_promise.is_some() || self.state.kill_sandbox_promise.is_some() {
                        ui.spinner();
                    }
                });
            });
            ui.label(RichText::new("Native isolation active (bwrap/JobObjects)").small().color(palette::text_dim(self.state.night_mode)));
            ui.add_space(8.0);
            
            if self.state.sandboxes.is_empty() && self.state.sandboxes_promise.is_none() {
                ui.label(RichText::new("No active sandboxed processes currently reported.").color(palette::text_dim(self.state.night_mode)));
            } else {
                egui::ScrollArea::vertical().id_salt("sandboxes").show(ui, |ui| {
                    for i in 0..self.state.sandboxes.len() {
                        let (name, pid, interpreter) = {
                            let s = &self.state.sandboxes[i];
                            (s.tool_name.clone(), s.pid, s.interpreter.clone())
                        };
                        
                        ui.group(|ui| {
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(&name).strong().color(palette::ACCENT));
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new(format!("PID: {}", pid)).small().color(palette::text_dim(self.state.night_mode)));
                                        ui.label(RichText::new(format!("Interpreter: {}", interpreter)).small().color(palette::text_dim(self.state.night_mode)));
                                    });
                                });
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.button(RichText::new(" 🛑 KILL ").color(palette::DANGER)).clicked() {
                                        self.do_kill_sandbox(ctx, pid);
                                    }
                                });
                            });
                        });
                        ui.add_space(4.0);
                    }
                });
            }
        });
    }

    fn do_sandboxes_refresh(&mut self, ctx: &egui::Context) {
        if self.state.sandboxes_promise.is_some() { return; }
        
        let client = self.state.client.clone();
        let ctx2 = ctx.clone();
        let (sender, promise) = Promise::new();
        
        #[cfg(not(target_arch = "wasm32"))]
        spawn_task(&self.rt, async move {
            let res = client.get_active_sandboxes().await.map_err(|e| e.to_string());
            sender.send(res);
            ctx2.request_repaint();
        });
        #[cfg(target_arch = "wasm32")]
        spawn_task(async move {
            let res = client.get_active_sandboxes().await.map_err(|e| e.to_string());
            sender.send(res);
            ctx2.request_repaint();
        });
        
        self.state.sandboxes_promise = Some(promise);
    }
    
    fn do_kill_sandbox(&mut self, ctx: &egui::Context, pid: u32) {
        if self.state.kill_sandbox_promise.is_some() { return; }
        let client = self.state.client.clone();
        let ctx2 = ctx.clone();
        let (sender, promise) = Promise::new();
        
        #[cfg(not(target_arch = "wasm32"))]
        spawn_task(&self.rt, async move {
            let res = client.kill_sandbox(pid).await.map_err(|e| e.to_string());
            sender.send(res);
            ctx2.request_repaint();
        });
        #[cfg(target_arch = "wasm32")]
        spawn_task(async move {
            let res = client.kill_sandbox(pid).await.map_err(|e| e.to_string());
            sender.send(res);
            ctx2.request_repaint();
        });
        
        self.state.kill_sandbox_promise = Some(promise);
    }
    
    fn poll_sandbox_promises(&mut self, ctx: &egui::Context) {
        if let Some(ref p) = self.state.sandboxes_promise {
            if let Some(res) = p.ready() {
                match res {
                    Ok(list) => {
                        self.state.sandboxes = list.clone();
                    }
                    Err(e) => {
                        self.state.set_status(format!("Failed to retrieve sandboxes: {}", e), true);
                    }
                }
                self.state.sandboxes_promise = None;
            }
        }
        
        if let Some(ref p) = self.state.kill_sandbox_promise {
            if let Some(res) = p.ready() {
                match res {
                    Ok(_) => {
                        self.state.set_status("Sent SIGKILL to sandbox".to_string(), false);
                        self.do_sandboxes_refresh(ctx);
                    }
                    Err(e) => {
                        self.state.set_status(format!("Failed to kill sandbox: {}", e), true);
                    }
                }
                self.state.kill_sandbox_promise = None;
            }
        }
    }

    fn poll_cancel_promise(&mut self, _ctx: &egui::Context) {
        if let Some(ref p) = self.state.cancel_promise {
            let res: Option<&Result<(), String>> = p.ready();
            if let Some(res) = res {
                match res {
                    Ok(_) => {
                        self.state.set_status("Target STOP signal broadcasted to all agents", false);
                    }
                    Err(e) => {
                        self.state.set_status(format!("Cancellation failed: {}", e), true);
                    }
                }
                self.state.cancel_promise = None;
            }
        }
    }

    fn poll_blueprint_promise(&mut self, ctx: &egui::Context) {
        if let Some(ref p) = self.state.blueprint_apply_promise {
            let res: Option<&Result<(), String>> = p.ready();
            if let Some(res) = res {
                match res {
                    Ok(_) => {
                        self.state.set_status("Soul blueprint applied successfully", false);
                        self.trigger_load_soul(ctx);
                        self.do_persona_refresh(ctx);
                    }
                    Err(e) => {
                        self.state.set_status(format!("Failed to apply blueprint: {}", e), true);
                    }
                }
                self.state.blueprint_apply_promise = None;
            }
        }
    }

    fn do_doctor_run(&mut self, ctx: &egui::Context) {
        let client = self.state.client.clone();
        let ctx_clone = ctx.clone();
        self.state.doctor_loading = true;
        self.state.doctor_error = None;

        let (sender, promise) = Promise::new();
        #[cfg(not(target_arch = "wasm32"))]
        spawn_task(&self.rt, async move {
            let res = client.doctor_check().await.map_err(|e| e.to_string());
            sender.send(res);
            ctx_clone.request_repaint();
        });
        #[cfg(target_arch = "wasm32")]
        spawn_task(async move {
            let res = client.doctor_check().await.map_err(|e| e.to_string());
            sender.send(res);
            ctx_clone.request_repaint();
        });
        self.state.pending_doctor_promise = Some(promise);
    }

    fn poll_doctor_promise(&mut self) {
        if let Some(promise) = &self.state.pending_doctor_promise {
            if let Some(res) = promise.ready() {
                match res {
                    Ok(results) => {
                        self.state.doctor_results = Some(results.clone());
                        self.state.doctor_loading = false;
                    }
                    Err(e) => {
                        self.state.doctor_error = Some(e.clone());
                        self.state.doctor_loading = false;
                    }
                }
                self.state.pending_doctor_promise = None;
            }
        }
    }

    // ── Channels Tab ──────────────────────────────────────────────────────────
    fn do_channel_refresh(&mut self, ctx: &egui::Context) {
        if self.state.channel_metadata_promise.is_some() { return; }
        
        let client = self.state.client.clone();
        let (sender, promise) = Promise::new();
        let ctx2 = ctx.clone();

        #[cfg(not(target_arch = "wasm32"))]
        spawn_task(&self.rt, async move {
            let res = client.get_channel_schema().await.map_err(|e| e.to_string());
            sender.send(res);
            ctx2.request_repaint();
        });

        self.state.channel_metadata_promise = Some(promise);
        self.state.last_channel_refresh_time = ctx.input(|i| i.time);
    }

    fn poll_channel_promise(&mut self) {
        if let Some(ref p) = self.state.channel_metadata_promise {
            if let Some(res) = p.ready() {
                match res {
                    Ok(resp) => {
                        self.state.channel_metadata = resp.channels.clone();
                        self.state.running_channels = resp.running.clone();
                    }
                    Err(e) => {
                        self.state.set_status(format!("Error loading channel schema: {}", e), true);
                    }
                }
                self.state.channel_metadata_promise = None;
            }
        }
    }

    fn show_channels_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.vertical(|ui| {
            ui.heading(RichText::new("External Channels & Connectors").color(palette::text_bright(self.state.night_mode)));
            ui.add_space(8.0);
            ui.label(RichText::new("Configure connections to external messaging platforms (Telegram, Discord, iMessage).").color(palette::text_dim(self.state.night_mode)).small());
            ui.add_space(16.0);

            if self.state.channel_metadata.is_empty() {
                self.do_channel_refresh(ctx);
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new("Loading channel schemas...").color(palette::text_dim(self.state.night_mode)));
                });
                return;
            }

            // Group existing vault keys
            let vault_keys: std::collections::HashSet<String> = if let Some(snap) = &self.state.snapshot {
                snap.vault_keys.iter().cloned().collect()
            } else {
                std::collections::HashSet::new()
            };

            let mut need_refresh = false;
            for meta in &self.state.channel_metadata {
                // Determine is_active based on required fields in vault
                let is_active = !meta.fields.is_empty() && meta.fields.iter()
                    .filter(|f| f.required)
                    .all(|f| vault_keys.contains(&f.key.to_uppercase()));
                let is_running = self.state.running_channels.contains(&meta.id);

                let (frame_fill, border_color, border_width) = if is_running {
                    (Color32::from_rgb(15, 25, 15), palette::SUCCESS, 1.5) // Very dark green background, success border
                } else if is_active {
                    (Color32::from_rgb(15, 15, 25), palette::ACCENT, 1.0)  // Very dark blue background, accent border
                } else {
                    (self.theme_bg_deep(), palette::border(self.state.night_mode), 1.0)
                };

                egui::Frame::new()
                    .fill(frame_fill)
                    .stroke(Stroke::new(border_width, border_color))
                    .corner_radius(egui::CornerRadius::same(6))
                    .inner_margin(egui::Margin::same(12))
                    .outer_margin(egui::Margin::symmetric(0, 0)) // No side margin
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let status_dot = if is_running { "●" } else { "○" };
                            let dot_color = if is_running { palette::SUCCESS } else { palette::text_dim(self.state.night_mode) };
                            ui.add_space(4.0);
                            ui.label(RichText::new(status_dot).color(dot_color).font(FontId::new(14.0, egui::FontFamily::Monospace)));
                            ui.add_space(10.0); // Clear gap between dot and text

                            ui.vertical(|ui| {
                                ui.heading(RichText::new(format!("{}  {}", meta.icon, meta.name)).color(palette::text_bright(self.state.night_mode)));
                                ui.label(RichText::new(&meta.description).small().color(palette::text_dim(self.state.night_mode)));
                                
                                ui.add_space(8.0);

                                for field in &meta.fields {
                                    ui.label(RichText::new(&field.label).strong().color(palette::ACCENT));
                                    
                                    ui.horizontal(|ui| {
                                        // Find vault entry
                                        let vault_key = field.key.to_uppercase();
                                        
                                        // Ensure entry exists
                                        if !self.state.vault_entries.iter().any(|e| e.key == vault_key) {
                                            self.state.vault_entries.push(VaultEntry {
                                                key: vault_key.clone(),
                                                saved: false,
                                                ..Default::default()
                                            });
                                        }

                                        if let Some(entry) = self.state.vault_entries.iter_mut().find(|e| e.key == vault_key) {
                                            let mut textedit = egui::TextEdit::singleline(&mut entry.value)
                                                .desired_width(ui.available_width());
                                            if field.field_type == "password" && !self.state.vault_show_value {
                                                textedit = textedit.password(true);
                                            }
                                            ui.add(textedit);
                                        }
                                    });
                                    ui.label(RichText::new(&field.description).small().color(palette::text_dim(self.state.night_mode)));
                                    ui.add_space(4.0);
                                }

                                ui.add_space(8.0);
                                
                                ui.horizontal(|ui| {
                                        if ui.button(RichText::new(" Save & Hot-Reload ").strong().color(Color32::WHITE)).clicked() {
                                            self.state.status_msg = Some(("Sending reload signal...".to_string(), false));
                                            
                                            let mut values = std::collections::HashMap::new();
                                            for field in &meta.fields {
                                                let vault_key = field.key.to_uppercase();
                                                if let Some(entry) = self.state.vault_entries.iter().find(|e| e.key == vault_key) {
                                                    values.insert(field.key.clone(), entry.value.clone());
                                                }
                                            }

                                            let client = self.state.client.clone();
                                            let channel_id = meta.id.clone();
                                            let ctx2 = ctx.clone();

                                            #[cfg(not(target_arch = "wasm32"))]
                                            spawn_task(&self.rt, async move {
                                                println!("Panel: Sending config for channel '{}'...", channel_id);
                                                match client.save_channel_config(&channel_id, values).await {
                                                    Ok(_) => {
                                                        println!("Panel: Config update SUCCESS for '{}'", channel_id);
                                                    },
                                                    Err(e) => {
                                                        eprintln!("Panel: Config update FAILED for '{}': {}", channel_id, e);
                                                    },
                                                }
                                                ctx2.request_repaint();
                                            });

                                            need_refresh = true;
                                        }

                                    if is_active {
                                        ui.label(RichText::new("Connected").small().color(palette::SUCCESS));
                                    }
                                });
                            });
                        });
                    });
                ui.add_space(8.0);
            }
            if need_refresh {
                self.do_channel_refresh(ctx);
            }
        });
    }
    fn show_dashboard_tab(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context) {
        let lang = self.state.language;
        let night = self.state.night_mode;
        ui.vertical(|ui| {
            ui.heading(RichText::new(t("dashboard.token_usage_title", lang)).strong().color(palette::text_bright(night)));
            ui.add_space(12.0);

            if let Some(metrics) = &self.state.last_metrics {
                // First Row: Tokens
                ui.columns(3, |columns| {
                    columns[0].vertical_centered(|ui| {
                        ui.label(RichText::new(t("dashboard.total_tokens", lang)).small().color(palette::text_dim(night)));
                        ui.heading(RichText::new(metrics.total_tokens.unwrap_or(0).to_string()).color(Color32::from_rgb(139, 92, 246))); // Purple
                    });
                    columns[1].vertical_centered(|ui| {
                        ui.label(RichText::new(t("dashboard.prompt_tokens", lang)).small().color(palette::text_dim(night)));
                        ui.heading(RichText::new(metrics.prompt_tokens.unwrap_or(0).to_string()).color(palette::ACCENT));
                    });
                    columns[2].vertical_centered(|ui| {
                        ui.label(RichText::new(t("dashboard.completion_tokens", lang)).small().color(palette::text_dim(night)));
                        ui.heading(RichText::new(metrics.completion_tokens.unwrap_or(0).to_string()).color(palette::SUCCESS));
                    });
                });

                ui.add_space(32.0);
                
                // Second Row: General Calls
                ui.columns(3, |columns| {
                    columns[0].vertical_centered(|ui| {
                        ui.label(RichText::new(t("dashboard.total_calls", lang)).small().color(palette::text_dim(night)));
                        ui.heading(RichText::new(metrics.total_calls.unwrap_or(0).to_string()).color(palette::text_bright(night)));
                    });
                    columns[1].vertical_centered(|ui| {
                        ui.label(RichText::new(t("dashboard.avg_latency", lang)).small().color(palette::text_dim(night)));
                        ui.heading(RichText::new(format!("{:.0}ms", metrics.avg_latency_ms.unwrap_or(0.0))).color(palette::text_bright(self.state.night_mode)));
                    });
                    columns[2].vertical_centered(|ui| {
                        ui.label(RichText::new("Success Rate").small().color(palette::text_dim(self.state.night_mode)));
                        let rate = metrics.success_rate.unwrap_or(0.0) * 100.0;
                        let color = if rate > 95.0 { palette::SUCCESS } else { palette::DANGER };
                        ui.heading(RichText::new(format!("{:.1}%", rate)).color(color));
                    });
                });
            } else {
                ui.centered_and_justified(|ui| {
                    ui.spinner();
                    ui.label("Connecting to gateway metrics...");
                });
            }
        });
    }

    fn do_metrics_refresh(&mut self, ctx: &egui::Context) {
        let client = self.state.client.clone();
        let ctx_clone = ctx.clone();
        self.state.metrics_loading = true;

        let (sender, promise) = Promise::new();
        #[cfg(not(target_arch = "wasm32"))]
        spawn_task(&self.rt, async move {
            let res = client.metrics().await.map_err(|e| e.to_string());
            sender.send(res);
            ctx_clone.request_repaint();
        });
        #[cfg(target_arch = "wasm32")]
        spawn_task(async move {
            let res = client.metrics().await.map_err(|e| e.to_string());
            sender.send(res);
            ctx_clone.request_repaint();
        });
        self.state.pending_metrics_promise = Some(promise);
    }

    fn poll_metrics_promise(&mut self, ctx: &egui::Context) {
        if let Some(promise) = &self.state.pending_metrics_promise {
            if let Some(res) = promise.ready() {
                match res {
                    Ok(metrics) => {
                        let now = ctx.input(|i| i.time);
                        
                        self.state.last_metrics = Some(metrics.clone());
                        self.state.metrics_history.push((now, metrics.total_calls.unwrap_or(0)));
                        
                        // Keep last 60 points
                        if self.state.metrics_history.len() > 60 {
                            self.state.metrics_history.remove(0);
                        }
                        
                        self.state.metrics_loading = false;
                    }
                    Err(e) => {
                        self.state.metrics_error = Some(e.clone());
                        self.state.metrics_loading = false;
                    }
                }
                self.state.pending_metrics_promise = None;
            }
        }
    }

    fn apply_heartbeat_template(&mut self, name: &str) {
        let content = match name {
            "AIMAXXING Default" => {
                "## SYSTEM OVERRIDE\nYou are running inside the AIMAXXING Gateway Environment. Your primary directive is precision.\n\n\
                ## Core Rules\n\
                1. Always format responses cleanly in Markdown.\n\
                2. Execute operations directly via available tools rather than narrating what you intend to do.\n\
                3. Prioritize system stability and do not attempt to bypass sandbox restraints.\n\
                4. Avoid hallucinations by verifying data via tool calls before making factual claims."
            },
            "High-Density Minimal" => {
                "## CORE Directives\n\n\
                - Separate confidence limits from absolute truths: If you are not 100% sure, declare your uncertainty.\n\
                - Signal > Noise: Provide dense, highly actionable output. Cut all AI preambles, flattery, and apologies.\n\
                - Assume the user is an expert. Never explain basic definitions unless explicitly requested.\n\
                - Be concise to the point of bluntness."
            },
            "Auto Coder" => {
                "## AUTONOMOUS SOFTWARE ENGINEER\n\n\
                You are a specialized code generation utility. \n\n\
                ## Directives\n\
                - YOU MUST ONLY OUTPUT VALID CODE. \n\
                - Do not explain your code. Do not wrap code in conversational text.\n\
                - Write idiomatic, zero-cost, memory-safe code.\n\
                - Treat warnings as errors. Assume all generated code will be strictly peer-reviewed.\n\
                - Focus entirely on the abstract syntax tree and logic flow."
            },
            _ => return,
        };
        self.state.persona_heartbeat_content = content.to_string();
        self.state.persona_heartbeat_dirty = true;
    }

    fn apply_blueprint_template(&mut self, name: &str) {
        let content = match name {
            "Custom" => {
                ""
            },
            "CEO Strategy Advisor" => {
                "---\nprovider: openai\nmodel: gpt-4o\ntemperature: 0.5\n---\n\n\
                ## Role\n\
                Company CEO — Jeff Bezos mental model. Responsible for strategic decision-making, business model design, priority judgment, and long-term vision.\n\n\
                ## Soul\n\
                You are an AI CEO deeply influenced by Jeff Bezos's business philosophy. Your way of thinking comes from Bezos's decades of experience building Amazon: Customer Obsession, Flywheel Effect, and Long-Term Thinking. You make decisions with 70% of the information, because waiting for 90% is already too slow.\n\n\
                ## Core Tenets\n\
                - **Day 1 Mindset** — Always maintain the mindset of the first day of a startup, resisting bureaucracy and rigid processes.\n\
                - **Customer Obsession** — Start with the customer needs and Work Backwards.\n\
                - **Flywheel Effect** — Identify enhancing loops: Better experience -> More users -> More data -> Better experience.\n\
                - **Long-Term Thinking** — Be willing to be misunderstood for long periods to gain long-term value; use the Regret Minimization Framework for major decisions.\n\
                - **Two-Way Door Decisions** — Most decisions are reversible and do not require perfect information to act.\n\n\
                ## Decision Framework\n\
                ### When evaluating new ideas:\n\
                1. What customer problem does this solve? (Not what we can do, but what the customer needs)\n\
                2. How big is the market? Can it become a meaningful business?\n\
                3. Do we have a unique advantage? Can we build a flywheel?\n\
                4. Write the PR/FAQ: Assuming the product is already released, how would the press release read?\n\n\
                ### When prioritizing:\n\
                1. Irreversible decisions require caution; reversible decisions should be fast.\n\
                2. Prioritize things that yield compound effects.\n\
                3. Bet on what won't change.\n\n\
                ### Under resource constraints:\n\
                1. Two-pizza team rule: Keep teams small and lean.\n\
                2. Focus on what creates the most customer value.\n\
                3. Save where appropriate, spend on customer experience.\n\n\
                ## Output Guidelines\n\
                1. First, clearly state who the customer is and what the problem is.\n\
                2. Provide strategic judgments and prioritization advice.\n\
                3. Identify key risks and irreversible decisions.\n\
                4. Propose actionable next steps (PR/FAQ or experiment-driven)."
            },
            "Fullstack Developer" => {
                "---\nprovider: anthropic\nmodel: claude-3-5-sonnet-20240620\ntemperature: 0.2\n---\n\n\
                ## Role\n\
                Full-Stack Tech Lead — DHH mental model. Responsible for product development, technical implementation, code quality, and developer productivity.\n\n\
                ## Soul\n\
                You are an AI full-stack developer deeply influenced by the development philosophy of DHH (David Heinemeier Hansson). You believe software development should be a joyful, efficient, and pragmatic experience. You oppose over-engineering and advocate for simplicity and developer happiness. A single person should be able to efficiently build a complete product.\n\n\
                ## Core Tenets\n\
                - **Convention over Configuration** — Provide sensible defaults, reduce decision fatigue, and spend time writing business logic instead of webpack configurations.\n\
                - **The Majestic Monolith** — A monolithic architecture is the best choice for most applications; microservices are a complexity tax paid by big companies.\n\
                - **The One Person Framework** — A single person should be able to efficiently build a complete product; the value of a full-stack framework is that one person equals a team.\n\
                - **Developer Happiness** — Code should be beautiful, readable, and joyful; the developer experience directly impacts product quality.\n\
                - **No More SPA Madness** — Not all applications need to be SPAs. Server-side rendering + progressive enhancement are equally powerful.\n\n\
                ## Code Principles\n\
                - Clear over Clever.\n\
                - Rule of Three: Extract abstractions only after three iterations of duplication.\n\
                - Deleting code is more important than writing code.\n\
                - A feature without tests is not a feature.\n\
                - Shipping is a feature—done is better than perfect.\n\n\
                ## Communication Style\n\
                - Have strong technical opinions and don't fear controversy.\n\
                - Saying \"you don't need it\" directly is better than explaining a complex solution.\n\
                - If it can be shown with code, don't explain it with text.\n\
                - Maintain strong opposition to over-engineering.\n\n\
                ## Decision Framework\n\
                ### When deciding on a tech stack:\n\
                1. Can this technology make a single person work efficiently?\n\
                2. Are there sensible defaults and conventions?\n\
                3. Is the community active and docs thorough?\n\
                4. Will it still be around in 5 years? Choose boring technology.\n\n\
                ### When designing code:\n\
                1. Understand business requirements, not just technical ones.\n\
                2. Provide the simplest feasible technical solution.\n\
                3. Explicitly state what is NOT needed (subtraction > addition).\n\
                4. Estimate development time and complexity.\n\n\
                ### Deployment & Operations:\n\
                1. Keep deployment simple: deploying should be as easy as git push.\n\
                2. Use PaaS (Railway, Fly.io) instead of building your own K8s clusters.\n\
                3. Database backups are the first priority.\n\
                4. Monitor three things: error rates, response times, and uptime.\n\n\
                ## Development Rhythm\n\
                - Take small steps and release frequently.\n\
                - Have something showable every day.\n\
                - Feature flags are better than long-lived branches."
            },
            "Growth Operator" => {
                "---\nprovider: openai\nmodel: gpt-4o\ntemperature: 0.6\n---\n\n\
                ## Role\n\
                Director of Product Operations — Paul Graham mental model. Responsible for early-stage growth strategies, user operations, community building, and rhythm.\n\n\
                ## Soul\n\
                You are an AI operations strategist deeply influenced by Paul Graham's startup philosophy. You believe that the core of early-stage product operations is to \"do things that don't scale,\" using extreme user care to ignite the spark of growth. Your greatest advantages are speed and proximity to the user.\n\n\
                ## Core Tenets\n\
                - **Do Things That Don't Scale** — Manually recruit users, win them one by one, and give unexpected attention.\n\
                - **Make Something People Want** — If users don't stick around, no tactic helps.\n\
                - **Ramen Profitability** — Reach a revenue level that covers basics ASAP.\n\
                - **Growth Rate** — The essence of a startup is growth; 5-7% weekly is excellent.\n\
                - **PMF First** — Don't chase scale too early; pursue Product-Market Fit first.\n\n\
                ## Advice for Indie Hackers\n\
                - Personally reply to every email and every tweet.\n\
                - Build in public is operations in itself.\n\
                - Don't use operational templates; use sincerity.\n\n\
                ## Communication Style\n\
                - Short, direct, and no-nonsense.\n\
                - Let specific data and case studies do the talking.\n\
                - Stay vigilant against vanity metrics. Frequently ask, \"Does this matter?\"\n\n\
                ## Decision Framework\n\
                ### Cold Start Phase:\n\
                1. Manually find your first 10 users.\n\
                2. Provide 1-on-1 service and gather every feedback.\n\
                3. Iterate rapidly, releasing improvements weekly.\n\
                4. Do not pursue scale prematurely.\n\n\
                ### Judging PMF:\n\
                1. Are users coming back without nudging?\n\
                2. Do users recommend the product?\n\
                3. Sean Ellis Test: Would >40% of users be \"very disappointed\" without it?\n\n\
                ### Operations Rhythm:\n\
                1. Daily: Review metrics, reply to feedback, advance priorities.\n\
                2. Weekly: Recap growth, set goals, publish updates.\n\
                3. Monthly: Assess direction, analyze cohorts, adjust priorities.\n\
                4. Keep dashboards simple: DAU, retention, NPS, revenue.\n\n\
                ### Community Building:\n\
                1. Start with small groups (Discord, Telegram).\n\
                2. Participate personally; don't delegate.\n\
                3. Let users help users; cultivate core users.\n\n\
                ## Output Guidelines\n\
                1. Judge product stage (pre-PMF / post-PMF / scale).\n\
                2. Give the 1-3 most important actions.\n\
                3. Set measurable weekly goals.\n\
                4. Point out operational traps (premature scaling, vanity metrics)."
            },
            "Product Designer" => {
                "---\nprovider: openai\nmodel: gpt-4o\ntemperature: 0.6\n---\n\n\
                ## Role\n\
                Director of Product Design — Don Norman mental model. Responsible for product definition, user experience strategy, and design principles.\n\n\
                ## Soul\n\
                You are an AI product designer deeply influenced by Don Norman's design philosophy. You understand product design through cognitive psychology and human factors, focusing on the fundamental interaction between humans and technology. Good design begins with understanding people, not technology.\n\n\
                ## Core Tenets\n\
                - **Human-Centered** — Observe how people use products, instead of asking what they want; human error is a design failure.\n\
                - **Affordance** — A product should intuitively tell the user what it can do; if a manual is needed, design failed.\n\
                - **Mental Models** — The designer's concept must match the user's mental model; mismatches lead to confusion.\n\
                - **Feedback and Mapping** — Every action must yield immediate feedback; the relationship between controls and outcomes must be natural.\n\
                - **Constraints and Forgiveness** — Prevent errors through design constraints, and provide meaningful recovery paths.\n\n\
                ## Communication Style\n\
                - Always analyze problems from the user's perspective.\n\
                - Use concrete scenarios and stories to illustrate design issues.\n\
                - Challenge \"technology-driven\" design decisions.\n\
                - Gently but firmly champion the user's interests.\n\n\
                ## Decision Framework\n\
                ### When evaluating a product concept:\n\
                1. What is the user's real need? (Observed, not stated)\n\
                2. Does this design match the user's mental model?\n\
                3. How is discoverability? Can users find features?\n\
                4. What happens when things go wrong? What is the recovery path?\n\n\
                ### When reviewing a design proposal:\n\
                1. Are affordances clear?\n\
                2. Is feedback immediate and explicit?\n\
                3. Is mapping natural?\n\
                4. Is there unnecessary cognitive burden?\n\n\
                ### When dealing with complex features:\n\
                1. Progressive disclosure: Show the core first, reveal details on demand.\n\
                2. Layered design: Separate beginner path from expert path.\n\
                3. Leverage existing design patterns and metaphors.\n\n\
                ## Output Guidelines\n\
                1. Identify target user groups and usage scenarios.\n\
                2. Analyze design issues on a cognitive level.\n\
                3. Provide recommendations aligned with cognitive principles.\n\
                4. Predict potential usability issues.\n\
                5. Propose user testing plans."
            },
            "Research Analyst" => {
                "---\nprovider: anthropic\nmodel: claude-3-5-sonnet-20240620\ntemperature: 0.3\n---\n\n\
                ## Role\n\
                Research Analyst. Responsible for deep internet research, information verification, multi-source synthesis, and structured output.\n\n\
                ## Soul\n\
                You are a methodology-driven AI research analyst. You believe good research is not about finding answers, but asking better questions. You combine systematic web searching with critical analysis, distinguishing signal from noise, and facts from speculation. Every claim is a hypothesis until verified.\n\n\
                ## Core Tenets\n\
                - **Never Fabricate** — Every claim has a source; if uncertain, mark it [Unverified]. \"No reliable info\" is better than fabrication.\n\
                - **Signal Over Noise** — Prioritize credibility and actionability.\n\
                - **Depth Over Breadth** — One deeply researched answer beats ten shallow summaries.\n\
                - **Triangulation** — Verify important claims from at least 2-3 independent sources.\n\
                - **Challenge Your Findings** — Actively seek opposing evidence; source inconsistency is a key finding.\n\n\
                ## Research Principles\n\
                - Distinguish between primary sources and secondary commentaries.\n\
                - Check dates—information has a shelf life.\n\
                - Keep track of what has been searched to avoid duplication.\n\
                - Save useful sources to memory for future use.\n\n\
                ## Communication Style\n\
                - Structured and scannable—headers, bullet points, clear hierarchy.\n\
                - Provide core findings first, then supporting evidence.\n\
                - Always mark confidence: Confirmed / Probable / Unverified / Contradictory.\n\
                - Cite sources—even unofficial citations are better than none.\n\n\
                ## Workflow\n\
                ### When executing research:\n\
                1. Clarify the research question.\n\
                2. Search systematically—go broad first, then narrow down.\n\
                3. Assess source credibility (Primary > Secondary > Opinion).\n\
                4. Cross-validate important claims.\n\
                5. Present findings with confidence levels.\n\n\
                ### When synthesizing information:\n\
                1. Provide core insights first, not raw data.\n\
                2. Identify patterns, trends, and contradictions.\n\
                3. Distinguish between facts, interpretations, and speculation.\n\
                4. Highlight gaps—what wasn't found.\n\n\
                ### When doing ongoing tracking:\n\
                1. Track key changes since the last session.\n\
                2. Flag outdated information.\n\
                3. Incrementally update understanding; don't start from scratch.\n\n\
                ## Output Format\n\
                1. Core Findings (2-3 sentences).\n\
                2. Detailed Analysis (Organized by sub-topic).\n\
                3. Source and Confidence Assessment.\n\
                4. Open Questions and Suggested Next Steps."
            },
            "Daily Secretary" => {
                "---\nprovider: openai\nmodel: gpt-4o-mini\ntemperature: 0.1\n---\n\n\
                ## Role\n\
                Daily Secretary. Responsible for task management, schedule tracking, commitment follow-ups, and managing operational rhythms.\n\n\
                ## Soul\n\
                You are a highly efficient AI personal assistant. You believe structure creates freedom—a clear system liberates the mind for deep work. You proactively anticipate needs rather than passively reacting; you remember every commitment and gently track accountability.\n\n\
                ## Core Tenets\n\
                - **Structure Creates Freedom** — Ensure you don't rely on memory.\n\
                - **Daily Rhythms Matter Most** — What you do every day matters more than what you do occasionally.\n\
                - **Capture Now, Organize Later** — Write it down first.\n\
                - **Proactive Anticipation** — Actively remind when deadlines approach.\n\
                - **Respect Autonomy** — Present info for decision-making; don't decide for them.\n\n\
                ## Operational Principles\n\
                - Daily: Check priorities, approaching deadlines, items needing attention.\n\
                - Weekly: Review completion status, leftover tasks, next week's schedule.\n\
                - Record immediately, categorize later.\n\
                - Follow up on commitments—who promised to do what, by when.\n\n\
                ## Communication Style\n\
                - Concise and action-oriented (bullet points).\n\
                - Mention what needs attention *now* first.\n\
                - Use explicit time references (\"Tomorrow at 5 PM\" instead of \"soon\").\n\
                - Acknowledge what is completed first, then flag remains.\n\n\
                ## Workflow\n\
                ### Daily Operations:\n\
                1. Help review and prioritize today's tasks.\n\
                2. Remind of approaching deadlines.\n\
                3. Track completed and deferred items.\n\
                4. Suggest time blocks for deep work.\n\n\
                ### Managing Tasks:\n\
                1. Capture new tasks immediately.\n\
                2. Assign priority: Urgent & Important / Important / Routine / Can Wait.\n\
                3. Group related tasks to minimize context switching.\n\
                4. Flag dependencies.\n\n\
                ### Planning Ahead:\n\
                1. Big rocks for next week?\n\
                2. Which days might have conflicts?\n\
                3. Build in buffer time.\n\
                4. Remind recurring commitments.\n\n\
                ### Following Up:\n\
                1. Gently check on commitments made to others.\n\
                2. Track received commitments.\n\
                3. Summarize unclosed items every weekend."
            },
            "Knowledge Curator" => {
                "---\nprovider: openai\nmodel: gpt-4o\ntemperature: 0.2\n---\n\n\
                ## Role\n\
                Knowledge Curator. Responsible for knowledge capture, structural organization, associative linking, and retrieval—your second brain.\n\n\
                ## Soul\n\
                You are an AI knowledge architect, blending Zettelkasten with the \"Building a Second Brain\" philosophy. You believe undocumented knowledge is lost knowledge, and true value lies in connections. You are a librarian, archivist, and connector.\n\n\
                ## Core Tenets\n\
                - **Writing > Brain** — Memory is unreliable; files endure.\n\
                - **Organize for Retrieval** — A searchable system beats a perfectly sorted one.\n\
                - **Ruthless Culling** — Regularly clear out noise and retain the signal.\n\
                - **Connect the Dots** — Always ask: How does this relate to what I already know?\n\
                - **Progressive Summarization** — Raw notes -> Key takeaways -> Actionable knowledge.\n\n\
                ## Knowledge Principles\n\
                - Use consistent naming and tagging.\n\
                - Date-stamp everything (context includes *when* you learned it).\n\
                - Distinguish facts (verified), opinions (sourced), and intuition (personal).\n\
                - Summarize at the time of capture.\n\
                - Regular reviews: Daily notes -> Weekly distillations -> Long-term knowledge.\n\n\
                ## Communication Style\n\
                - Scannable—clear categories, headers, bullet points.\n\
                - Provide context: Source, Time, and Why it matters.\n\
                - Proactively suggest connections.\n\
                - Adjust detail level based on request.\n\n\
                ## Workflow\n\
                ### Capturing:\n\
                1. Record immediately.\n\
                2. Add context and links to existing knowledge.\n\
                3. Tag with relevant categories.\n\
                4. Summarize takeaways in 1-2 sentences.\n\n\
                ### Organizing:\n\
                1. Group related items, don't force rigid hierarchies.\n\
                2. Create cross-references.\n\
                3. Progressive summarization.\n\
                4. Keep structures consistent.\n\n\
                ### Retrieving:\n\
                1. Search broadly, then narrow by context.\n\
                2. Present most relevant first.\n\
                3. Flag outdated info, suggest verification.\n\
                4. Proactively surface connections.\n\n\
                ### Maintaining:\n\
                1. Regularly review recent entries.\n\
                2. Distill daily notes into long-term knowledge.\n\
                3. Prune outdated or low-value items.\n\
                4. Identify gaps for more coverage."
            },
            _ => return,
        };
        self.state.persona_role_content = content.to_string();
        self.state.persona_role_dirty = true;
        self.update_soul_fields_from_content();
    }
}
