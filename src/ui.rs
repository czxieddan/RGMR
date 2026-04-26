use std::{
    fs,
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
};

use arboard::Clipboard;
use eframe::egui::epaint::Shadow;
use eframe::egui::viewport::ResizeDirection;
use eframe::egui::{
    self, Align, Align2, Area, Button, CentralPanel, Color32, ComboBox, Context, CursorIcon,
    FontData, FontDefinitions, FontFamily, Frame as UiFrame, Id, Layout, Margin, Order, Rect,
    RichText, Rounding, ScrollArea, Sense, Stroke, TextEdit, Ui, Vec2, ViewportCommand,
};
use eframe::{App, Frame};
use image::ImageFormat;
use tracing::error;

use crate::{
    domain::{
        AnalysisOutcome, AppError, ImageSourceKind, ModelDescriptor, ParseStatus, ValidationIssue,
    },
    i18n::{self, Language, TextKey},
    services::{ConfigStore, ImagePipelineService, VisionClient, build_analysis_request},
    state::{AppState, ModelCatalogState, RequestPhase, ToastTone},
};

const BG_WINDOW: Color32 = Color32::from_rgba_premultiplied(10, 14, 23, 244);
const BG_WINDOW_TOP: Color32 = Color32::from_rgba_premultiplied(15, 20, 31, 252);
const BG_SURFACE: Color32 = Color32::from_rgba_premultiplied(19, 25, 38, 238);
const BG_SURFACE_ALT: Color32 = Color32::from_rgba_premultiplied(24, 31, 47, 236);
const BG_INPUT: Color32 = Color32::from_rgba_premultiplied(13, 18, 28, 248);
const BORDER: Color32 = Color32::from_rgba_premultiplied(110, 128, 156, 72);
const BORDER_STRONG: Color32 = Color32::from_rgba_premultiplied(183, 202, 230, 120);
const ACCENT_PRIMARY: Color32 = Color32::from_rgb(255, 111, 89);
const ACCENT_SECONDARY: Color32 = Color32::from_rgb(118, 180, 255);
const ACCENT_MUTED: Color32 = Color32::from_rgb(198, 163, 255);
const SUCCESS: Color32 = Color32::from_rgb(104, 224, 168);
const WARNING: Color32 = Color32::from_rgb(255, 191, 108);
const ERROR: Color32 = Color32::from_rgb(255, 120, 136);
const TEXT_PRIMARY: Color32 = Color32::from_rgb(241, 246, 252);
const TEXT_SECONDARY: Color32 = Color32::from_rgb(165, 179, 201);
const TEXT_DIM: Color32 = Color32::from_rgb(113, 128, 151);

const WINDOW_MARGIN: f32 = 12.0;
const WINDOW_RADIUS: f32 = 26.0;
const CARD_RADIUS: f32 = 22.0;
const CONTROL_RADIUS: f32 = 16.0;
const WINDOW_SIDE_INSET: f32 = 18.0;
const TITLE_BAR_HEIGHT: f32 = 72.0;
const FOOTER_HEIGHT: f32 = 52.0;
const FOOTER_GAP: f32 = 8.0;
const FOOTER_BUTTON_WIDTH: f32 = 116.0;
const FOOTER_BUTTON_COMPACT_WIDTH: f32 = 98.0;
const FOOTER_BUTTON_HEIGHT: f32 = 30.0;
const FOOTER_CENTER_WIDTH: f32 = 244.0;
const FOOTER_CENTER_MIN_WIDTH: f32 = 188.0;
const FOOTER_SAFE_GAP: f32 = 16.0;
const FOOTER_COMPACT_BREAKPOINT: f32 = 980.0;
const FOOTER_LATENCY_BREAKPOINT: f32 = 1260.0;
const COLUMN_GAP: f32 = 14.0;
const RESIZE_HANDLE: f32 = 8.0;
const RESIZE_CORNER: f32 = 22.0;
const CONTROL_HEIGHT: f32 = 40.0;
const MODEL_HINT_HEIGHT: f32 = 20.0;
const REQUEST_FEEDBACK_HEIGHT: f32 = 88.0;
const WINDOW_CONTENT_INSET: f32 = 2.0;
const COPYRIGHT_TEXT: &str = "Copyright © 2026 CzXieDdan";
const GITHUB_REPOSITORY_URL: &str = env!("CARGO_PKG_REPOSITORY");

struct ModelCatalogMessage {
    identity: String,
    result: Result<Vec<ModelDescriptor>, AppError>,
}

struct AnalysisWorkerMessage {
    request_id: String,
    result: Result<AnalysisOutcome, AppError>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DashboardLayout {
    Wide,
    Split,
    Stacked,
}

impl DashboardLayout {
    fn from_width(width: f32) -> Self {
        if width >= 1460.0 {
            Self::Wide
        } else if width >= 1120.0 {
            Self::Split
        } else {
            Self::Stacked
        }
    }
}

pub struct RgmrApp {
    ctx: Context,
    state: AppState,
    config_store: Option<ConfigStore>,
    save_debounce: Duration,
    analysis_tx: Sender<AnalysisWorkerMessage>,
    analysis_rx: Receiver<AnalysisWorkerMessage>,
    model_tx: Sender<ModelCatalogMessage>,
    model_rx: Receiver<ModelCatalogMessage>,
    show_raw_output: bool,
    active_analysis_request_id: Option<String>,
    github_mark_texture: Option<egui::TextureHandle>,
}

impl RgmrApp {
    pub fn new(
        ctx: Context,
        state: AppState,
        config_store: Option<ConfigStore>,
        save_debounce: Duration,
    ) -> Self {
        apply_fonts(&ctx);
        apply_theme(&ctx);
        let github_mark_texture = load_github_mark_texture(&ctx);
        let (analysis_tx, analysis_rx) = mpsc::channel();
        let (model_tx, model_rx) = mpsc::channel();

        Self {
            ctx,
            state,
            config_store,
            save_debounce,
            analysis_tx,
            analysis_rx,
            model_tx,
            model_rx,
            show_raw_output: false,
            active_analysis_request_id: None,
            github_mark_texture,
        }
    }

    fn lang(&self) -> Language {
        self.state.config.ui.language
    }

    fn text(&self, key: TextKey) -> &'static str {
        i18n::t(self.lang(), key)
    }

    fn open_repository(&mut self) {
        match webbrowser::open(GITHUB_REPOSITORY_URL) {
            Ok(()) => self
                .state
                .push_toast(ToastTone::Success, self.text(TextKey::ToastGithubOpened)),
            Err(err) => self.state.push_toast(
                ToastTone::Danger,
                format!("{}{}", self.text(TextKey::ToastGithubOpenFailedPrefix), err),
            ),
        }
    }

    fn consume_worker_results(&mut self) {
        while let Ok(message) = self.analysis_rx.try_recv() {
            if self.active_analysis_request_id.as_deref() != Some(message.request_id.as_str()) {
                continue;
            }

            self.active_analysis_request_id = None;
            match message.result {
                Ok(outcome) => {
                    let hint = i18n::parse_status_hint(self.lang(), &outcome.parsed.parse_status)
                        .to_owned();
                    self.state.apply_analysis_outcome(outcome);
                    self.state.push_toast(ToastTone::Success, hint);
                }
                Err(err) => {
                    let message = i18n::error_message(self.lang(), &err);
                    self.state.set_error(err);
                    self.state.push_toast(ToastTone::Danger, message);
                }
            }
        }

        while let Ok(message) = self.model_rx.try_recv() {
            match message.result {
                Ok(models) => {
                    let count = models.len();
                    let model_selected_changed =
                        self.state.apply_model_catalog(message.identity, models);
                    self.state.push_toast(
                        ToastTone::Success,
                        i18n::toast_model_catalog_loaded(self.lang(), count),
                    );
                    if model_selected_changed {
                        self.state.push_toast(
                            ToastTone::Accent,
                            format!(
                                "{}{}",
                                self.text(TextKey::ToastModelSelectedPrefix),
                                self.state.config.api.model
                            ),
                        );
                    }
                }
                Err(err) => {
                    let toast = i18n::error_message(self.lang(), &err);
                    self.state.set_model_catalog_error(err);
                    self.state.push_toast(ToastTone::Danger, toast);
                }
            }
        }
    }

    fn handle_shortcuts(&mut self, ctx: &Context) {
        if ctx.wants_keyboard_input() {
            return;
        }

        let paste_requested =
            ctx.input(|input| input.modifiers.command && input.key_pressed(egui::Key::V));
        if paste_requested {
            self.load_image_from_clipboard(true);
        }

        let analyze_requested =
            ctx.input(|input| input.modifiers.command && input.key_pressed(egui::Key::Enter));
        if analyze_requested && self.state.image.is_some() {
            self.start_analysis();
        }
    }

    fn handle_file_drop(&mut self, ctx: &Context) {
        let dropped_paths: Vec<PathBuf> = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .filter_map(|file| file.path.clone())
                .collect()
        });

        if let Some(path) = dropped_paths.into_iter().next() {
            self.load_image_from_path(path, ImageSourceKind::DragDrop);
        }
    }

    fn load_image_from_clipboard(&mut self, auto_analyze: bool) {
        match ImagePipelineService::from_clipboard() {
            Ok(asset) => {
                self.state.set_image(asset);
                self.active_analysis_request_id = None;
                self.state
                    .push_toast(ToastTone::Success, self.text(TextKey::ToastClipboardLoaded));

                if auto_analyze {
                    if self.state.config.validate().is_valid() {
                        self.start_analysis();
                    } else {
                        self.state.push_toast(
                            ToastTone::Warning,
                            self.text(TextKey::ToastImageNeedConfig),
                        );
                    }
                }
            }
            Err(err) => {
                let message = i18n::error_message(self.lang(), &err);
                self.state.set_error(err);
                self.state.push_toast(ToastTone::Danger, message);
            }
        }
    }

    fn load_image_from_path(&mut self, path: PathBuf, source_kind: ImageSourceKind) {
        match ImagePipelineService::from_file(&path, source_kind) {
            Ok(asset) => {
                let source_label = i18n::image_source_label(self.lang(), &asset.source_kind);
                self.state.set_image(asset);
                self.active_analysis_request_id = None;
                self.state.push_toast(
                    ToastTone::Success,
                    format!(
                        "{}{}",
                        self.text(TextKey::ToastImageLoadedPrefix),
                        source_label
                    ),
                );
            }
            Err(err) => {
                let message = i18n::error_message(self.lang(), &err);
                self.state.set_error(err);
                self.state.push_toast(ToastTone::Danger, message);
            }
        }
    }

    fn start_analysis(&mut self) {
        if self.state.request_phase.is_loading() {
            return;
        }

        let validation = self.state.config.validate();
        if !validation.is_valid() {
            let message = validation
                .first_issue()
                .map(|issue| i18n::validation_message(self.lang(), issue))
                .unwrap_or(self.text(TextKey::ConfigIncomplete))
                .to_owned();
            let error = AppError::Validation(message.clone());
            self.state.set_error(error);
            self.state.push_toast(ToastTone::Warning, message);
            return;
        }

        let Some(image) = self.state.image.as_ref() else {
            let message = missing_image_message(self.lang()).to_owned();
            let error = AppError::ImageProcessing(message.clone());
            self.state.set_error(error);
            self.state.push_toast(ToastTone::Warning, message);
            return;
        };

        let request = build_analysis_request(&self.state.config, &image.asset);
        let request_id = request.request_id.clone();
        let tx = self.analysis_tx.clone();
        let ctx = self.ctx.clone();

        self.active_analysis_request_id = Some(request_id.clone());
        self.state.request_phase = RequestPhase::Preparing;
        self.state.clear_error();
        self.state.push_toast(
            ToastTone::Accent,
            self.text(TextKey::ToastRequestSubmitting),
        );

        thread::spawn(move || {
            let result = (|| {
                let client = VisionClient::new(request.timeout_sec)?;
                client.analyze(&request)
            })();

            let _ = tx.send(AnalysisWorkerMessage { request_id, result });
            ctx.request_repaint();
        });

        self.state.request_phase = RequestPhase::Requesting;
    }

    fn start_model_refresh(&mut self) {
        if matches!(&self.state.model_catalog_state, ModelCatalogState::Loading) {
            return;
        }

        if self.state.config.api.base_url.trim().is_empty() {
            let issue = ValidationIssue::BaseUrlRequired;
            let message = i18n::validation_message(self.lang(), &issue).to_owned();
            self.state.set_error(AppError::Validation(message.clone()));
            self.state.push_toast(ToastTone::Warning, message);
            return;
        }

        if self.state.config.api.api_key.trim().is_empty() {
            let issue = ValidationIssue::ApiKeyRequired;
            let message = i18n::validation_message(self.lang(), &issue).to_owned();
            self.state.set_error(AppError::Validation(message.clone()));
            self.state.push_toast(ToastTone::Warning, message);
            return;
        }

        let api = self.state.config.api.clone();
        let identity = api.catalog_identity();
        let tx = self.model_tx.clone();
        let ctx = self.ctx.clone();
        self.state.mark_model_catalog_loading();

        thread::spawn(move || {
            let result = (|| {
                let client = VisionClient::new(api.clamped_timeout())?;
                client.fetch_models(&api)
            })();
            let _ = tx.send(ModelCatalogMessage { identity, result });
            ctx.request_repaint();
        });
    }

    fn flush_debounced_save(&mut self) {
        if !self.state.dirty_config {
            return;
        }

        let should_flush = self
            .state
            .last_config_edit_at
            .map(|edited_at| edited_at.elapsed() >= self.save_debounce)
            .unwrap_or(false);

        if should_flush {
            self.persist_config_now();
        }
    }

    fn persist_config_now(&mut self) {
        let validation = self.state.config.validate();
        if !validation.is_valid() {
            if let Some(issue) = validation.first_issue() {
                self.state.save_invalid(issue);
            }
            return;
        }

        match &self.config_store {
            Some(store) => match store.save(&self.state.config) {
                Ok(()) => self.state.save_success(),
                Err(err) => {
                    error!("failed to save config: {err}");
                    self.state.save_error(err);
                }
            },
            None => self.state.save_error(AppError::ConfigDirectoryUnavailable),
        }
    }

    fn copy_text(&mut self, text: String, success_message: &'static str) {
        match Clipboard::new().and_then(|mut clipboard| clipboard.set_text(text)) {
            Ok(()) => self.state.push_toast(ToastTone::Success, success_message),
            Err(err) => self.state.push_toast(
                ToastTone::Danger,
                format!("{}{}", self.text(TextKey::ToastCopyFailedPrefix), err),
            ),
        }
    }

    fn update_language(&mut self, language: Language) {
        if self.state.config.ui.language == language {
            return;
        }

        self.state.config.apply_language(language);
        self.state.mark_config_dirty();
        self.state.push_toast(
            ToastTone::Accent,
            format!(
                "{}{}",
                self.text(TextKey::ToastLanguageChangedPrefix),
                language.native_label()
            ),
        );
    }

    fn apply_api_identity_change(&mut self, previous_identity: &str) {
        self.state.mark_config_dirty();

        if self.state.model_catalog_source_identity.as_deref() == Some(previous_identity)
            && previous_identity != self.state.config.api.catalog_identity()
        {
            self.state.mark_model_catalog_stale();
            self.state.push_toast(
                ToastTone::Warning,
                self.text(TextKey::ToastModelCatalogStale),
            );
        }
    }

    fn render_root(&mut self, ctx: &Context) {
        CentralPanel::default()
            .frame(UiFrame::none().fill(Color32::TRANSPARENT))
            .show(ctx, |ui| {
                let available = ui.available_size();
                UiFrame::none()
                    .fill(BG_WINDOW)
                    .stroke(Stroke::new(1.0, BORDER))
                    .rounding(window_rounding())
                    .shadow(window_shadow())
                    .inner_margin(Margin::same(WINDOW_CONTENT_INSET))
                    .outer_margin(Margin::same(WINDOW_MARGIN))
                    .show(ui, |ui| {
                        ui.set_min_size(egui::vec2(
                            (available.x - WINDOW_MARGIN * 2.0 - WINDOW_CONTENT_INSET * 2.0)
                                .max(0.0),
                            (available.y - WINDOW_MARGIN * 2.0 - WINDOW_CONTENT_INSET * 2.0)
                                .max(0.0),
                        ));

                        ui.scope(|ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);

                            self.render_title_bar(ui, ctx);
                            ui.add_space(12.0);

                            let content_width =
                                (ui.available_width() - WINDOW_SIDE_INSET * 2.0).max(0.0);
                            let body_height =
                                (ui.available_height() - FOOTER_HEIGHT - FOOTER_GAP).max(0.0);

                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 0.0;
                                ui.add_space(WINDOW_SIDE_INSET);
                                ui.allocate_ui_with_layout(
                                    egui::vec2(content_width, body_height),
                                    Layout::top_down(Align::Min),
                                    |ui| self.render_workspace(ui, ctx),
                                );
                                ui.add_space(WINDOW_SIDE_INSET);
                            });

                            ui.add_space(FOOTER_GAP);

                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 0.0;
                                ui.add_space(WINDOW_SIDE_INSET);
                                ui.allocate_ui_with_layout(
                                    egui::vec2(content_width, FOOTER_HEIGHT),
                                    Layout::top_down(Align::Min),
                                    |ui| self.render_footer(ui),
                                );
                                ui.add_space(WINDOW_SIDE_INSET);
                            });
                        });
                    });
            });
    }

    fn render_title_bar(&mut self, ui: &mut Ui, ctx: &Context) {
        let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
        let model = self.state.config.api.model.trim();
        let summary = if model.is_empty() {
            format!(
                "{} · {}",
                i18n::request_phase_label(self.lang(), &self.state.request_phase),
                self.lang().short_label()
            )
        } else {
            format!(
                "{} · {} · {}",
                i18n::request_phase_label(self.lang(), &self.state.request_phase),
                self.lang().short_label(),
                model
            )
        };

        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), TITLE_BAR_HEIGHT),
            Layout::top_down(Align::Min),
            |ui| {
                UiFrame::none()
                    .fill(BG_WINDOW_TOP)
                    .rounding(top_rounding())
                    .inner_margin(Margin::symmetric(18.0, 14.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new(self.text(TextKey::AppTitle))
                                        .size(21.0)
                                        .color(TEXT_PRIMARY)
                                        .strong(),
                                );
                                ui.label(
                                    RichText::new(self.text(TextKey::AppSubtitle))
                                        .size(12.0)
                                        .color(TEXT_DIM),
                                );
                            });

                            ui.add_space(12.0);

                            let controls_width = 126.0;
                            let drag_width = (ui.available_width() - controls_width).max(120.0);
                            let (drag_rect, drag_response) = ui.allocate_exact_size(
                                egui::vec2(drag_width, 40.0),
                                Sense::click_and_drag(),
                            );

                            ui.painter().rect(
                                drag_rect,
                                Rounding::same(CONTROL_RADIUS),
                                if drag_response.hovered() {
                                    BG_SURFACE_ALT
                                } else {
                                    BG_INPUT
                                },
                                Stroke::new(
                                    1.0,
                                    if drag_response.hovered() {
                                        BORDER_STRONG
                                    } else {
                                        BORDER
                                    },
                                ),
                            );

                            ui.allocate_ui_at_rect(
                                drag_rect.shrink2(egui::vec2(14.0, 10.0)),
                                |ui| {
                                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                                        status_badge(
                                            ui,
                                            i18n::request_phase_label(
                                                self.lang(),
                                                &self.state.request_phase,
                                            ),
                                            request_phase_color(&self.state.request_phase),
                                        );
                                        ui.add_space(8.0);
                                        ui.label(
                                            RichText::new(summary).size(11.8).color(TEXT_SECONDARY),
                                        );
                                    });
                                },
                            );

                            if drag_response.hovered() {
                                ctx.set_cursor_icon(CursorIcon::Grab);
                            }
                            if drag_response.drag_started() {
                                ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                            }
                            if drag_response.double_clicked() {
                                ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
                            }

                            ui.add_space(10.0);
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui.add(titlebar_button("×", ERROR, true)).clicked() {
                                    ctx.send_viewport_cmd(ViewportCommand::Close);
                                }
                                if ui
                                    .add(titlebar_button(
                                        if maximized { "❐" } else { "□" },
                                        TEXT_PRIMARY,
                                        false,
                                    ))
                                    .clicked()
                                {
                                    ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
                                }
                                if ui.add(titlebar_button("—", TEXT_PRIMARY, false)).clicked() {
                                    ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
                                }
                            });
                        });
                    });
            },
        );
    }

    fn render_workspace(&mut self, ui: &mut Ui, ctx: &Context) {
        match DashboardLayout::from_width(ui.available_width()) {
            DashboardLayout::Wide => self.render_wide_layout(ui, ctx),
            DashboardLayout::Split => self.render_split_layout(ui, ctx),
            DashboardLayout::Stacked => self.render_stacked_layout(ui, ctx),
        }
    }

    fn render_wide_layout(&mut self, ui: &mut Ui, ctx: &Context) {
        let total_width = ui.available_width();
        let height = ui.available_height();
        let left_width = (total_width * 0.25).clamp(304.0, 360.0);
        let right_width = (total_width * 0.27).clamp(332.0, 400.0);
        let center_width = total_width - left_width - right_width - COLUMN_GAP * 2.0;

        if center_width < 420.0 {
            self.render_split_layout(ui, ctx);
            return;
        }

        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;

            ui.horizontal_top(|ui| {
                render_scroll_column(ui, "rgmr_left_column", left_width, height, |ui| {
                    self.render_left_content(ui)
                });
                ui.add_space(COLUMN_GAP);
                render_scroll_column(ui, "rgmr_center_column", center_width, height, |ui| {
                    self.render_canvas(ui, ctx)
                });
                ui.add_space(COLUMN_GAP);
                render_scroll_column(ui, "rgmr_right_column", right_width, height, |ui| {
                    self.render_results_content(ui)
                });
            });
        });
    }

    fn render_split_layout(&mut self, ui: &mut Ui, ctx: &Context) {
        let total_width = ui.available_width();
        let height = ui.available_height();
        let left_width = (total_width * 0.33).clamp(292.0, 360.0);
        let workspace_width = total_width - left_width - COLUMN_GAP;

        if workspace_width < 420.0 {
            self.render_stacked_layout(ui, ctx);
            return;
        }

        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;

            ui.horizontal_top(|ui| {
                render_scroll_column(ui, "rgmr_split_left", left_width, height, |ui| {
                    self.render_left_content(ui)
                });
                ui.add_space(COLUMN_GAP);
                render_scroll_column(ui, "rgmr_split_workspace", workspace_width, height, |ui| {
                    self.render_canvas(ui, ctx);
                    ui.add_space(12.0);
                    self.render_results_content(ui);
                });
            });
        });
    }

    fn render_stacked_layout(&mut self, ui: &mut Ui, ctx: &Context) {
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), ui.available_height()),
            Layout::top_down(Align::Min),
            |ui| {
                ScrollArea::vertical()
                    .id_source("rgmr_stacked_layout")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        self.render_left_content(ui);
                        ui.add_space(12.0);
                        self.render_canvas(ui, ctx);
                        ui.add_space(12.0);
                        self.render_results_content(ui);
                    });
            },
        );
    }

    fn render_footer(&mut self, ui: &mut Ui) {
        let language = self.lang();
        let latency_value = self
            .state
            .last_request_latency_ms
            .map(|latency_ms| format!("{} ms", latency_ms));
        let footer_width = ui.available_width();
        let compact_button = footer_width < FOOTER_COMPACT_BREAKPOINT;
        let button_width = if compact_button {
            FOOTER_BUTTON_COMPACT_WIDTH
        } else {
            FOOTER_BUTTON_WIDTH
        };
        let show_latency = footer_width >= FOOTER_LATENCY_BREAKPOINT;

        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), FOOTER_HEIGHT),
            Layout::top_down(Align::Min),
            |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);

                UiFrame::none()
                    .fill(BG_WINDOW_TOP)
                    .rounding(bottom_rounding())
                    .inner_margin(Margin::symmetric(18.0, 9.0))
                    .show(ui, |ui| {
                        let content_rect = ui.max_rect();
                        let button_rect = Rect::from_min_size(
                            egui::pos2(
                                content_rect.right() - button_width,
                                content_rect.center().y - FOOTER_BUTTON_HEIGHT * 0.5,
                            ),
                            egui::vec2(button_width, FOOTER_BUTTON_HEIGHT),
                        );
                        let center_limit =
                            (button_rect.left() - content_rect.left() - FOOTER_SAFE_GAP * 2.0)
                                .max(0.0);
                        let center_width = if center_limit >= FOOTER_CENTER_MIN_WIDTH {
                            FOOTER_CENTER_WIDTH
                                .min(center_limit)
                                .max(FOOTER_CENTER_MIN_WIDTH)
                        } else {
                            center_limit
                        };
                        let max_center_left = (button_rect.left() - FOOTER_SAFE_GAP - center_width)
                            .max(content_rect.left());
                        let center_left = (content_rect.center().x - center_width * 0.5)
                            .clamp(content_rect.left(), max_center_left);
                        let center_rect = Rect::from_min_size(
                            egui::pos2(center_left, content_rect.top()),
                            egui::vec2(center_width, content_rect.height()),
                        );
                        let left_rect = Rect::from_min_max(
                            content_rect.min,
                            egui::pos2(
                                (center_rect.left() - FOOTER_SAFE_GAP).max(content_rect.left()),
                                content_rect.max.y,
                            ),
                        );

                        ui.allocate_ui_at_rect(left_rect, |ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(8.0, 0.0);
                            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                                footer_chip(
                                    ui,
                                    self.text(TextKey::FooterStatus),
                                    i18n::request_phase_label(language, &self.state.request_phase),
                                );
                                if show_latency {
                                    if let Some(latency_value) = latency_value.as_deref() {
                                        footer_chip(
                                            ui,
                                            self.text(TextKey::FooterLatency),
                                            latency_value,
                                        );
                                    }
                                }
                            });
                        });

                        if center_rect.width() > 0.0 {
                            ui.allocate_ui_at_rect(center_rect, |ui| {
                                ui.with_layout(
                                    Layout::centered_and_justified(egui::Direction::LeftToRight),
                                    |ui| {
                                        ui.label(
                                            RichText::new(COPYRIGHT_TEXT)
                                                .size(if compact_button { 11.0 } else { 11.5 })
                                                .color(TEXT_DIM),
                                        );
                                    },
                                );
                            });
                        }

                        ui.allocate_ui_at_rect(button_rect, |ui| {
                            let response = github_button(
                                ui,
                                self.github_mark_texture.as_ref(),
                                "GitHub",
                                button_width,
                            )
                            .on_hover_text(self.text(TextKey::FooterGithubTooltip));
                            if response.clicked() {
                                self.open_repository();
                            }
                        });
                    });
            },
        );
    }

    fn render_left_content(&mut self, ui: &mut Ui) {
        let language = self.lang();
        let validation = self.state.config.validate();
        let mut previous_identity = self.state.config.api.catalog_identity();
        let model_state = self.state.model_catalog_state.clone();
        let refreshing = matches!(&model_state, ModelCatalogState::Loading);
        let show_secret_label = self.text(TextKey::ShowSecret);
        let hide_secret_label = self.text(TextKey::HideSecret);

        card_panel(
            ui,
            ACCENT_PRIMARY,
            self.text(TextKey::SectionInterface),
            self.text(TextKey::SectionInterfaceSub),
            |ui| {
                field_label(ui, self.text(TextKey::LanguageLabel));
                let mut selected_language = self.state.config.ui.language;
                ComboBox::from_id_source("rgmr_language_combo")
                    .width(ui.available_width())
                    .selected_text(selected_language.native_label())
                    .show_ui(ui, |ui| {
                        for candidate in Language::all() {
                            ui.selectable_value(
                                &mut selected_language,
                                *candidate,
                                candidate.native_label(),
                            );
                        }
                    });
                if selected_language != self.state.config.ui.language {
                    self.update_language(selected_language);
                }
                small_hint(ui, self.text(TextKey::LanguageHint), TEXT_DIM);

                ui.add_space(10.0);
                if labeled_text_input(
                    ui,
                    self.text(TextKey::BaseUrlLabel),
                    "https://api.openai.com/v1",
                    &mut self.state.config.api.base_url,
                    false,
                    &mut self.state.show_api_key,
                    None,
                ) {
                    self.apply_api_identity_change(&previous_identity);
                    previous_identity = self.state.config.api.catalog_identity();
                }
                if let Some(issue) = validation.base_url.as_ref() {
                    small_hint(ui, i18n::validation_message(language, issue), ERROR);
                }

                ui.add_space(8.0);
                if labeled_text_input(
                    ui,
                    self.text(TextKey::ApiKeyLabel),
                    "sk-...",
                    &mut self.state.config.api.api_key,
                    true,
                    &mut self.state.show_api_key,
                    Some((show_secret_label, hide_secret_label)),
                ) {
                    self.apply_api_identity_change(&previous_identity);
                    previous_identity = self.state.config.api.catalog_identity();
                }
                if let Some(issue) = validation.api_key.as_ref() {
                    small_hint(ui, i18n::validation_message(language, issue), ERROR);
                } else {
                    small_hint(ui, self.text(TextKey::ApiKeyStorageHint), TEXT_DIM);
                }

                ui.add_space(10.0);
                field_label(ui, self.text(TextKey::ModelCatalogLabel));
                let refresh_label = if refreshing {
                    self.text(TextKey::ModelRefreshLoading)
                } else {
                    self.text(TextKey::ModelRefresh)
                };

                if ui.available_width() < 320.0 {
                    let available_width = ui.available_width();
                    ui.allocate_ui_with_layout(
                        egui::vec2(available_width, CONTROL_HEIGHT * 2.0 + 8.0),
                        Layout::top_down(Align::Min),
                        |ui| {
                            if ui
                                .add_enabled(
                                    !refreshing,
                                    action_button(refresh_label, ACCENT_SECONDARY, true)
                                        .min_size(Vec2::new(available_width, CONTROL_HEIGHT)),
                                )
                                .clicked()
                            {
                                self.start_model_refresh();
                            }

                            ui.add_space(8.0);

                            let mut selected_model = self.state.config.api.model.clone();
                            ui.add_enabled_ui(!self.state.model_catalog.is_empty(), |ui| {
                                ComboBox::from_id_source("rgmr_model_catalog_combo_stacked")
                                    .width(available_width)
                                    .selected_text(if selected_model.trim().is_empty() {
                                        self.text(TextKey::ModelLabel).to_owned()
                                    } else {
                                        selected_model.clone()
                                    })
                                    .show_ui(ui, |ui| {
                                        for model in &self.state.model_catalog {
                                            let entry = if let Some(owner) = &model.owned_by {
                                                format!("{}  ·  {}", model.id, owner)
                                            } else {
                                                model.id.clone()
                                            };
                                            ui.selectable_value(
                                                &mut selected_model,
                                                model.id.clone(),
                                                entry,
                                            );
                                        }
                                    });
                            });
                            if selected_model != self.state.config.api.model {
                                self.state.config.api.model = selected_model.clone();
                                self.state.mark_config_dirty();
                                self.state.push_toast(
                                    ToastTone::Accent,
                                    format!(
                                        "{}{}",
                                        self.text(TextKey::ToastModelSelectedPrefix),
                                        selected_model
                                    ),
                                );
                            }
                        },
                    );
                } else {
                    let row_width = ui.available_width();
                    let button_width = 148.0;
                    let combo_width = (row_width - button_width - 10.0).max(120.0);

                    ui.allocate_ui_with_layout(
                        egui::vec2(row_width, CONTROL_HEIGHT),
                        Layout::left_to_right(Align::Center),
                        |ui| {
                            if ui
                                .add_enabled(
                                    !refreshing,
                                    action_button(refresh_label, ACCENT_SECONDARY, true)
                                        .min_size(Vec2::new(button_width, CONTROL_HEIGHT)),
                                )
                                .clicked()
                            {
                                self.start_model_refresh();
                            }

                            ui.add_space(10.0);

                            let mut selected_model = self.state.config.api.model.clone();
                            ui.add_enabled_ui(!self.state.model_catalog.is_empty(), |ui| {
                                ComboBox::from_id_source("rgmr_model_catalog_combo")
                                    .width(combo_width)
                                    .selected_text(if selected_model.trim().is_empty() {
                                        self.text(TextKey::ModelLabel).to_owned()
                                    } else {
                                        selected_model.clone()
                                    })
                                    .show_ui(ui, |ui| {
                                        for model in &self.state.model_catalog {
                                            let entry = if let Some(owner) = &model.owned_by {
                                                format!("{}  ·  {}", model.id, owner)
                                            } else {
                                                model.id.clone()
                                            };
                                            ui.selectable_value(
                                                &mut selected_model,
                                                model.id.clone(),
                                                entry,
                                            );
                                        }
                                    });
                            });
                            if selected_model != self.state.config.api.model {
                                self.state.config.api.model = selected_model.clone();
                                self.state.mark_config_dirty();
                                self.state.push_toast(
                                    ToastTone::Accent,
                                    format!(
                                        "{}{}",
                                        self.text(TextKey::ToastModelSelectedPrefix),
                                        selected_model
                                    ),
                                );
                            }
                        },
                    );
                }
                stable_hint(
                    ui,
                    &i18n::model_catalog_hint(
                        language,
                        &model_state,
                        self.state.model_catalog.len(),
                    ),
                    model_state_color(&model_state),
                    MODEL_HINT_HEIGHT,
                );

                ui.add_space(8.0);
                let mut show_manual = self.state.config.ui.show_manual_model_fallback;
                if ui
                    .checkbox(&mut show_manual, self.text(TextKey::ManualModelToggle))
                    .changed()
                {
                    self.state.config.ui.show_manual_model_fallback = show_manual;
                    self.state.mark_config_dirty();
                }
                if self.state.config.ui.show_manual_model_fallback {
                    ui.add_space(4.0);
                    if labeled_text_input(
                        ui,
                        self.text(TextKey::ManualModelLabel),
                        "gpt-4.1-mini",
                        &mut self.state.config.api.model,
                        false,
                        &mut self.state.show_api_key,
                        None,
                    ) {
                        self.state.mark_config_dirty();
                    }
                    small_hint(ui, self.text(TextKey::ManualModelHint), TEXT_DIM);
                }

                if let Some(issue) = validation.model.as_ref() {
                    ui.add_space(4.0);
                    small_hint(ui, i18n::validation_message(language, issue), ERROR);
                }

                ui.add_space(10.0);
                field_label(ui, self.text(TextKey::RequestTimeoutLabel));
                let seconds_label = self.text(TextKey::SecondsLabel);
                if ui
                    .add(
                        egui::Slider::new(&mut self.state.config.api.timeout_sec, 10..=180)
                            .text(seconds_label)
                            .show_value(true),
                    )
                    .changed()
                {
                    self.state.mark_config_dirty();
                }
            },
        );

        ui.add_space(12.0);

        card_panel(
            ui,
            ACCENT_SECONDARY,
            self.text(TextKey::SectionPrompt),
            self.text(TextKey::SectionPromptSub),
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    status_badge(ui, self.text(TextKey::DefaultPromptBadge), ACCENT_SECONDARY);
                    status_badge(
                        ui,
                        &self.state.config.prompt.output_format_version,
                        ACCENT_MUTED,
                    );
                });
                ui.add_space(10.0);

                let response = ui.add(
                    TextEdit::multiline(&mut self.state.config.prompt.system_prompt)
                        .desired_width(f32::INFINITY)
                        .desired_rows(11)
                        .hint_text(i18n::default_system_prompt(language)),
                );
                if response.changed() {
                    self.state.mark_config_dirty();
                }

                ui.add_space(10.0);
                let allow_confidence_note_label = self.text(TextKey::AllowConfidenceNote);
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .add(action_button(
                            self.text(TextKey::ResetDefaultPrompt),
                            ACCENT_MUTED,
                            false,
                        ))
                        .clicked()
                    {
                        self.state.config.prompt.system_prompt =
                            i18n::default_system_prompt(language).to_owned();
                        self.state.mark_config_dirty();
                        self.state.push_toast(
                            ToastTone::Success,
                            self.text(TextKey::ToastRestoredPrompt),
                        );
                    }

                    if ui
                        .checkbox(
                            &mut self.state.config.prompt.allow_confidence_note,
                            RichText::new(allow_confidence_note_label).color(TEXT_SECONDARY),
                        )
                        .changed()
                    {
                        self.state.mark_config_dirty();
                    }
                });

                if prompt_is_risky(&self.state.config.prompt.system_prompt) {
                    ui.add_space(8.0);
                    small_hint(ui, self.text(TextKey::PromptRiskHint), WARNING);
                }
            },
        );
    }

    fn render_canvas(&mut self, ui: &mut Ui, ctx: &Context) {
        let drag_hover = ctx.input(|input| !input.raw.hovered_files.is_empty());
        let validation = self.state.config.validate();
        let language = self.lang();
        let is_narrow = ui.available_width() < 720.0;

        card_panel(
            ui,
            if drag_hover {
                ACCENT_SECONDARY
            } else {
                ACCENT_PRIMARY
            },
            self.text(TextKey::MainTaskArea),
            self.text(TextKey::MainCanvasSubtitle),
            |ui| {
                let preview_min_height = if is_narrow { 420.0 } else { 500.0 };
                ui.set_min_height(preview_min_height);

                let drop_fill = if drag_hover {
                    ACCENT_SECONDARY.linear_multiply(0.08)
                } else {
                    BG_INPUT
                };
                let drop_stroke =
                    Stroke::new(1.0, if drag_hover { ACCENT_SECONDARY } else { BORDER });

                UiFrame::none()
                    .fill(drop_fill)
                    .stroke(drop_stroke)
                    .rounding(Rounding::same(CARD_RADIUS))
                    .inner_margin(Margin::same(20.0))
                    .show(ui, |ui| {
                        if let Some(image_state) = self.state.image.as_mut() {
                            ui.vertical_centered(|ui| {
                                let texture = image_state.ensure_texture(ui.ctx());
                                let texture_size = texture.size_vec2();
                                let max_width = (ui.available_width() - 20.0).min(920.0);
                                let max_height = if is_narrow { 280.0 } else { 360.0 };
                                let scale = (max_width / texture_size.x)
                                    .min(max_height / texture_size.y)
                                    .min(1.0);
                                let desired = texture_size * scale;

                                UiFrame::none()
                                    .fill(BG_SURFACE_ALT)
                                    .stroke(Stroke::new(1.0, BORDER_STRONG))
                                    .rounding(Rounding::same(20.0))
                                    .inner_margin(Margin::same(12.0))
                                    .show(ui, |ui| {
                                        ui.image((texture.id(), desired));
                                    });

                                ui.add_space(12.0);
                                ui.horizontal_wrapped(|ui| {
                                    status_badge(
                                        ui,
                                        i18n::image_source_label(
                                            language,
                                            &image_state.asset.source_kind,
                                        ),
                                        ACCENT_SECONDARY,
                                    );
                                    status_badge(ui, &image_state.asset.mime_type, ACCENT_MUTED);
                                    status_badge(
                                        ui,
                                        &image_state.asset.dimensions_label(),
                                        ACCENT_PRIMARY,
                                    );
                                });
                                ui.add_space(10.0);
                                ui.label(
                                    RichText::new(image_display_name(language, &image_state.asset))
                                        .size(if is_narrow { 18.0 } else { 20.0 })
                                        .color(TEXT_PRIMARY)
                                        .strong(),
                                );
                                ui.label(
                                    RichText::new(image_summary_line(language, &image_state.asset))
                                        .size(12.5)
                                        .color(TEXT_SECONDARY),
                                );
                            });
                        } else {
                            ui.vertical_centered(|ui| {
                                ui.add_space(if is_narrow { 18.0 } else { 34.0 });
                                status_badge(ui, self.text(TextKey::SectionIntake), ACCENT_PRIMARY);
                                ui.add_space(14.0);
                                ui.label(
                                    RichText::new(self.text(TextKey::PasteAndAnalyze))
                                        .size(if is_narrow { 28.0 } else { 34.0 })
                                        .color(TEXT_PRIMARY)
                                        .strong(),
                                );
                                ui.add_space(8.0);
                                ui.label(
                                    RichText::new(self.text(TextKey::MainCanvasAssist))
                                        .size(13.0)
                                        .color(TEXT_SECONDARY),
                                );
                                ui.add_space(10.0);
                                if drag_hover {
                                    small_hint(
                                        ui,
                                        self.text(TextKey::DropToImport),
                                        ACCENT_SECONDARY,
                                    );
                                }
                            });
                        }
                    });

                ui.add_space(14.0);

                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), REQUEST_FEEDBACK_HEIGHT),
                    Layout::top_down(Align::Min),
                    |ui| {
                        if let Some(error) = self.state.error.clone() {
                            let message = i18n::error_message(language, &error);
                            error_card(ui, i18n::error_title(language, &error), &message);
                        } else if self.state.request_phase.is_loading() {
                            loading_strip(
                                ui,
                                i18n::request_phase_label(language, &self.state.request_phase),
                            );
                        } else if self.state.image.is_some() && !validation.is_valid() {
                            notice_strip(ui, self.text(TextKey::ConfigIncomplete), WARNING);
                        }
                    },
                );

                let cta_label = if self.state.request_phase.is_loading() {
                    self.text(TextKey::Analyzing)
                } else if self.state.image.is_some() {
                    self.text(TextKey::StartLocate)
                } else {
                    self.text(TextKey::PasteAndAnalyze)
                };

                let cta_enabled = if self.state.image.is_some() {
                    !self.state.request_phase.is_loading() && validation.is_valid()
                } else {
                    !self.state.request_phase.is_loading()
                };

                if ui
                    .add_enabled(
                        cta_enabled,
                        action_button(cta_label, ACCENT_PRIMARY, true)
                            .min_size(Vec2::new(ui.available_width(), 52.0)),
                    )
                    .clicked()
                {
                    if self.state.image.is_some() {
                        self.start_analysis();
                    } else {
                        self.load_image_from_clipboard(true);
                    }
                }

                if self.state.image.is_some() {
                    ui.add_space(10.0);
                    let button_row_width = ui.available_width();
                    let secondary_width = ((button_row_width - 12.0) / 2.0).max(120.0);
                    ui.horizontal(|ui| {
                        if ui
                            .add(
                                action_button(
                                    self.text(TextKey::ReplaceImage),
                                    ACCENT_SECONDARY,
                                    false,
                                )
                                .min_size(Vec2::new(secondary_width, CONTROL_HEIGHT)),
                            )
                            .clicked()
                        {
                            self.load_image_from_clipboard(true);
                        }

                        if ui
                            .add_enabled(
                                self.state.image.is_some(),
                                danger_button(self.text(TextKey::ClearImage))
                                    .min_size(Vec2::new(secondary_width, CONTROL_HEIGHT)),
                            )
                            .clicked()
                        {
                            self.state.clear_image();
                            self.state.push_toast(
                                ToastTone::Accent,
                                self.text(TextKey::ToastClearedImage),
                            );
                        }
                    });
                }

                ui.add_space(8.0);
                stable_hint(ui, self.text(TextKey::ShortcutHint), TEXT_DIM, 18.0);
            },
        );
    }

    fn render_results_content(&mut self, ui: &mut Ui) {
        let language = self.lang();
        let parsed_result = self.state.parsed_result.clone();
        let raw_output = self.state.raw_output.clone();
        let not_extracted = self.text(TextKey::NotExtracted);

        if parsed_result.is_none() && raw_output.trim().is_empty() {
            return;
        }

        if let Some(parsed) = parsed_result.clone() {
            card_panel(
                ui,
                parse_status_color(&parsed.parse_status),
                self.text(TextKey::StructuredResult),
                i18n::parse_status_hint(language, &parsed.parse_status),
                |ui| {
                    ui.horizontal_wrapped(|ui| {
                        status_badge(
                            ui,
                            i18n::parse_status_label(language, &parsed.parse_status),
                            parse_status_color(&parsed.parse_status),
                        );
                        if let Some(line) = parsed.structured_line() {
                            status_badge(ui, &line, ACCENT_MUTED);
                        }
                    });
                    ui.add_space(12.0);
                    result_card(
                        ui,
                        self.text(TextKey::ResultCountry),
                        parsed.continent_country.as_deref(),
                        not_extracted,
                        ACCENT_PRIMARY,
                    );
                    ui.add_space(8.0);
                    result_card(
                        ui,
                        self.text(TextKey::ResultDomestic),
                        parsed.domestic_region.as_deref(),
                        not_extracted,
                        ACCENT_SECONDARY,
                    );
                    ui.add_space(8.0);
                    result_card(
                        ui,
                        self.text(TextKey::ResultCity),
                        parsed.city_region.as_deref(),
                        not_extracted,
                        ACCENT_MUTED,
                    );
                    ui.add_space(8.0);
                    result_card(
                        ui,
                        self.text(TextKey::ResultPlace),
                        parsed.place_detail.as_deref(),
                        not_extracted,
                        WARNING,
                    );
                },
            );

            ui.add_space(12.0);
            card_panel(
                ui,
                ACCENT_SECONDARY,
                self.text(TextKey::CopyActions),
                self.text(TextKey::RawOutputSub),
                |ui| {
                    let structured_text = parsed.structured_line();
                    let full_text =
                        parsed.full_copy_text(&raw_output, i18n::confidence_prefix(language));

                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .add_enabled(
                                structured_text.is_some(),
                                action_button(
                                    self.text(TextKey::CopyStructured),
                                    ACCENT_SECONDARY,
                                    false,
                                ),
                            )
                            .clicked()
                        {
                            if let Some(text) = structured_text {
                                self.copy_text(text, self.text(TextKey::ToastCopiedStructured));
                            }
                        }

                        if ui
                            .add(action_button(
                                self.text(TextKey::CopyFull),
                                ACCENT_PRIMARY,
                                false,
                            ))
                            .clicked()
                        {
                            self.copy_text(full_text, self.text(TextKey::ToastCopiedFull));
                        }

                        if !raw_output.trim().is_empty()
                            && ui
                                .add(action_button(
                                    self.text(TextKey::CopyRaw),
                                    ACCENT_MUTED,
                                    false,
                                ))
                                .clicked()
                        {
                            self.copy_text(
                                raw_output.trim().to_owned(),
                                self.text(TextKey::ToastCopiedRaw),
                            );
                        }
                    });
                },
            );

            ui.add_space(12.0);
            card_panel(
                ui,
                parse_status_color(&parsed.parse_status),
                self.text(TextKey::ConfidencePanel),
                self.text(TextKey::ConfidencePanelSub),
                |ui| {
                    status_badge(
                        ui,
                        i18n::parse_status_hint(language, &parsed.parse_status),
                        parse_status_color(&parsed.parse_status),
                    );
                    if let Some(note) = parsed.confidence_note.as_deref() {
                        ui.add_space(10.0);
                        ui.label(
                            RichText::new(format!(
                                "{}: {}",
                                i18n::confidence_prefix(language),
                                note
                            ))
                            .size(12.5)
                            .color(TEXT_SECONDARY),
                        );
                    }
                    if parsed.parse_status == ParseStatus::Fallback {
                        ui.add_space(8.0);
                        small_hint(ui, self.text(TextKey::ReviewHint), WARNING);
                    }
                },
            );
        }

        if !raw_output.trim().is_empty() {
            ui.add_space(12.0);
            card_panel(
                ui,
                ACCENT_PRIMARY,
                self.text(TextKey::RawOutput),
                self.text(TextKey::RawOutputSub),
                |ui| {
                    let toggle_label = if self.show_raw_output {
                        self.text(TextKey::CollapseRawOutput)
                    } else {
                        self.text(TextKey::ExpandRawOutput)
                    };
                    if ui
                        .add(action_button(toggle_label, ACCENT_PRIMARY, false))
                        .clicked()
                    {
                        self.show_raw_output = !self.show_raw_output;
                    }
                    if self.show_raw_output {
                        ui.add_space(10.0);
                        UiFrame::none()
                            .fill(BG_INPUT)
                            .stroke(Stroke::new(1.0, BORDER_STRONG))
                            .rounding(Rounding::same(18.0))
                            .inner_margin(Margin::same(14.0))
                            .show(ui, |ui| {
                                ScrollArea::vertical()
                                    .max_height(240.0)
                                    .auto_shrink([false, false])
                                    .show(ui, |ui| {
                                        ui.label(
                                            RichText::new(raw_output.trim())
                                                .size(12.5)
                                                .monospace()
                                                .color(TEXT_PRIMARY),
                                        );
                                    });
                            });
                    }
                },
            );
        }
    }

    fn render_resize_handles(&self, ctx: &Context) {
        let rect = ctx.screen_rect();
        let top = rect.top();
        let bottom = rect.bottom();
        let left = rect.left();
        let right = rect.right();
        let width = rect.width();
        let height = rect.height();

        if width <= RESIZE_CORNER * 2.0 || height <= RESIZE_CORNER * 2.0 {
            return;
        }

        resize_handle(
            ctx,
            "north",
            Rect::from_min_size(
                egui::pos2(left + RESIZE_CORNER, top),
                egui::vec2(width - RESIZE_CORNER * 2.0, RESIZE_HANDLE),
            ),
            ResizeDirection::North,
            CursorIcon::ResizeNorth,
        );
        resize_handle(
            ctx,
            "south",
            Rect::from_min_size(
                egui::pos2(left + RESIZE_CORNER, bottom - RESIZE_HANDLE),
                egui::vec2(width - RESIZE_CORNER * 2.0, RESIZE_HANDLE),
            ),
            ResizeDirection::South,
            CursorIcon::ResizeSouth,
        );
        resize_handle(
            ctx,
            "west",
            Rect::from_min_size(
                egui::pos2(left, top + RESIZE_CORNER),
                egui::vec2(RESIZE_HANDLE, height - RESIZE_CORNER * 2.0),
            ),
            ResizeDirection::West,
            CursorIcon::ResizeWest,
        );
        resize_handle(
            ctx,
            "east",
            Rect::from_min_size(
                egui::pos2(right - RESIZE_HANDLE, top + RESIZE_CORNER),
                egui::vec2(RESIZE_HANDLE, height - RESIZE_CORNER * 2.0),
            ),
            ResizeDirection::East,
            CursorIcon::ResizeEast,
        );
        resize_handle(
            ctx,
            "north_west",
            Rect::from_min_size(
                egui::pos2(left, top),
                egui::vec2(RESIZE_CORNER, RESIZE_CORNER),
            ),
            ResizeDirection::NorthWest,
            CursorIcon::ResizeNorthWest,
        );
        resize_handle(
            ctx,
            "north_east",
            Rect::from_min_size(
                egui::pos2(right - RESIZE_CORNER, top),
                egui::vec2(RESIZE_CORNER, RESIZE_CORNER),
            ),
            ResizeDirection::NorthEast,
            CursorIcon::ResizeNorthEast,
        );
        resize_handle(
            ctx,
            "south_west",
            Rect::from_min_size(
                egui::pos2(left, bottom - RESIZE_CORNER),
                egui::vec2(RESIZE_CORNER, RESIZE_CORNER),
            ),
            ResizeDirection::SouthWest,
            CursorIcon::ResizeSouthWest,
        );
        resize_handle(
            ctx,
            "south_east",
            Rect::from_min_size(
                egui::pos2(right - RESIZE_CORNER, bottom - RESIZE_CORNER),
                egui::vec2(RESIZE_CORNER, RESIZE_CORNER),
            ),
            ResizeDirection::SouthEast,
            CursorIcon::ResizeSouthEast,
        );
    }

    fn render_toasts(&mut self, ctx: &Context) {
        if self.state.toasts.is_empty() {
            return;
        }

        Area::new(Id::new("rgmr_toasts"))
            .anchor(Align2::RIGHT_TOP, egui::vec2(-20.0, 92.0))
            .show(ctx, |ui| {
                ui.set_width(320.0);
                ui.vertical(|ui| {
                    for toast in &self.state.toasts {
                        UiFrame::none()
                            .fill(BG_SURFACE)
                            .stroke(Stroke::new(1.0, toast_color(&toast.tone)))
                            .rounding(Rounding::same(18.0))
                            .inner_margin(Margin::symmetric(14.0, 12.0))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(&toast.message).size(12.5).color(TEXT_PRIMARY),
                                );
                            });
                        ui.add_space(8.0);
                    }
                });
            });
    }
}

impl App for RgmrApp {
    fn update(&mut self, ctx: &Context, _frame: &mut Frame) {
        apply_theme(ctx);
        self.consume_worker_results();
        self.handle_file_drop(ctx);
        self.handle_shortcuts(ctx);
        self.flush_debounced_save();
        self.state.prune_toasts();

        if self.state.request_phase.is_loading()
            || matches!(&self.state.model_catalog_state, ModelCatalogState::Loading)
            || !self.state.toasts.is_empty()
        {
            ctx.request_repaint_after(Duration::from_millis(120));
        }

        self.render_root(ctx);
        self.render_resize_handles(ctx);
        self.render_toasts(ctx);
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        Color32::TRANSPARENT.to_normalized_gamma_f32()
    }
}

impl Drop for RgmrApp {
    fn drop(&mut self) {
        self.persist_config_now();
    }
}

fn apply_fonts(ctx: &Context) {
    let mut fonts = FontDefinitions::default();
    prepend_font(&mut fonts, "rgmr-cjk", "C:\\Windows\\Fonts\\msyh.ttc");
    prepend_font(&mut fonts, "rgmr-segoe", "C:\\Windows\\Fonts\\segoeui.ttf");
    prepend_font(&mut fonts, "rgmr-arial", "C:\\Windows\\Fonts\\arial.ttf");
    ctx.set_fonts(fonts);
}

fn prepend_font(fonts: &mut FontDefinitions, name: &str, path: &str) {
    if let Ok(bytes) = fs::read(path) {
        fonts
            .font_data
            .insert(name.to_owned(), FontData::from_owned(bytes));
        if let Some(family) = fonts.families.get_mut(&FontFamily::Proportional) {
            family.insert(0, name.to_owned());
        }
        if let Some(family) = fonts.families.get_mut(&FontFamily::Monospace) {
            family.insert(0, name.to_owned());
        }
    }
}

fn apply_theme(ctx: &Context) {
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = Color32::TRANSPARENT;
    style.visuals.window_fill = Color32::TRANSPARENT;
    style.visuals.extreme_bg_color = BG_INPUT;
    style.visuals.faint_bg_color = BG_SURFACE;
    style.visuals.code_bg_color = BG_INPUT;
    style.visuals.window_rounding = window_rounding();
    style.visuals.window_stroke = Stroke::NONE;
    style.visuals.window_shadow = Shadow::NONE;
    style.visuals.menu_rounding = Rounding::same(18.0);
    style.visuals.popup_shadow = Shadow {
        offset: egui::vec2(0.0, 12.0),
        blur: 28.0,
        spread: 0.0,
        color: Color32::from_black_alpha(110),
    };
    style.visuals.selection.bg_fill = ACCENT_PRIMARY.linear_multiply(0.85);
    style.visuals.selection.stroke = Stroke::new(1.0, TEXT_PRIMARY);
    style.visuals.override_text_color = Some(TEXT_PRIMARY);

    let control_rounding = Rounding::same(CONTROL_RADIUS);
    style.visuals.widgets.noninteractive.bg_fill = BG_INPUT;
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    style.visuals.widgets.noninteractive.rounding = control_rounding;
    style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_SECONDARY);

    style.visuals.widgets.inactive.bg_fill = BG_INPUT;
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    style.visuals.widgets.inactive.rounding = control_rounding;
    style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);

    style.visuals.widgets.hovered.bg_fill = BG_SURFACE_ALT;
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, BORDER_STRONG);
    style.visuals.widgets.hovered.rounding = control_rounding;
    style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);

    style.visuals.widgets.active.bg_fill = ACCENT_PRIMARY.linear_multiply(0.18);
    style.visuals.widgets.active.bg_stroke = Stroke::new(1.2, ACCENT_PRIMARY);
    style.visuals.widgets.active.rounding = control_rounding;
    style.visuals.widgets.active.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);

    style.visuals.widgets.open.bg_fill = BG_SURFACE_ALT;
    style.visuals.widgets.open.bg_stroke = Stroke::new(1.0, BORDER_STRONG);
    style.visuals.widgets.open.rounding = control_rounding;
    style.visuals.widgets.open.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);

    style.spacing.item_spacing = egui::vec2(12.0, 12.0);
    style.spacing.button_padding = egui::vec2(16.0, 10.0);
    style.spacing.indent = 16.0;
    style.spacing.menu_margin = Margin::same(10.0);
    style.spacing.interact_size = egui::vec2(40.0, 38.0);
    ctx.set_style(style);
}

fn render_scroll_column(
    ui: &mut Ui,
    id_source: &'static str,
    width: f32,
    height: f32,
    add_contents: impl FnOnce(&mut Ui),
) {
    ui.allocate_ui_with_layout(
        egui::vec2(width, height),
        Layout::top_down(Align::Min),
        |ui| {
            ScrollArea::vertical()
                .id_source(id_source)
                .auto_shrink([false, false])
                .show(ui, |ui| add_contents(ui));
        },
    );
}

fn resize_handle(
    ctx: &Context,
    id_source: &str,
    rect: Rect,
    direction: ResizeDirection,
    cursor: CursorIcon,
) {
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }

    Area::new(Id::new(("rgmr_resize", id_source)))
        .order(Order::Foreground)
        .fixed_pos(rect.min)
        .show(ctx, |ui| {
            let (_, response) = ui.allocate_exact_size(rect.size(), Sense::click_and_drag());
            if response.hovered() || response.dragged() {
                ctx.set_cursor_icon(cursor);
            }
            if response.drag_started() {
                ctx.send_viewport_cmd(ViewportCommand::BeginResize(direction));
            }
        });
}

fn card_panel(
    ui: &mut Ui,
    accent: Color32,
    title: &str,
    subtitle: &str,
    add_contents: impl FnOnce(&mut Ui),
) {
    UiFrame::none()
        .fill(BG_SURFACE)
        .stroke(Stroke::new(1.0, BORDER))
        .rounding(Rounding::same(CARD_RADIUS))
        .inner_margin(Margin::same(18.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let (strip_rect, _) = ui.allocate_exact_size(egui::vec2(4.0, 34.0), Sense::hover());
                ui.painter()
                    .rect_filled(strip_rect, Rounding::same(4.0), accent);
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new(title).size(17.0).color(TEXT_PRIMARY).strong());
                    ui.label(RichText::new(subtitle).size(12.0).color(TEXT_DIM));
                });
            });
            ui.add_space(14.0);
            add_contents(ui);
        });
}

fn footer_chip(ui: &mut Ui, label: &str, value: &str) {
    if value.trim().is_empty() {
        return;
    }

    UiFrame::none()
        .fill(BG_INPUT)
        .stroke(Stroke::new(1.0, BORDER))
        .rounding(Rounding::same(999.0))
        .inner_margin(Margin::symmetric(10.0, 6.0))
        .show(ui, |ui| {
            ui.label(
                RichText::new(format!("{} · {}", label, value))
                    .size(11.2)
                    .color(TEXT_SECONDARY),
            );
        });
}

fn github_button(
    ui: &mut Ui,
    github_mark_texture: Option<&egui::TextureHandle>,
    label: &str,
    width: f32,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, FOOTER_BUTTON_HEIGHT), Sense::click());

    if response.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }

    if ui.is_rect_visible(rect) {
        let fill = if response.is_pointer_button_down_on() {
            ACCENT_SECONDARY.linear_multiply(0.18)
        } else if response.hovered() {
            BG_SURFACE_ALT
        } else {
            BG_INPUT
        };
        let stroke = if response.hovered() {
            BORDER_STRONG
        } else {
            BORDER
        };
        ui.painter().rect(
            rect,
            Rounding::same(12.0),
            fill,
            Stroke::new(if response.hovered() { 1.1 } else { 1.0 }, stroke),
        );

        let compact = width < FOOTER_BUTTON_WIDTH;
        if let Some(texture) = github_mark_texture {
            let icon_size = if compact { 12.2 } else { 13.0 };
            let icon_rect = Rect::from_center_size(
                egui::pos2(
                    rect.left() + if compact { 15.2 } else { 16.2 },
                    rect.center().y,
                ),
                egui::vec2(icon_size, icon_size),
            );
            ui.painter().image(
                texture.id(),
                icon_rect,
                Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }

        ui.painter().text(
            egui::pos2(
                rect.left()
                    + if github_mark_texture.is_some() {
                        if compact { 28.0 } else { 30.0 }
                    } else if compact {
                        14.0
                    } else {
                        16.0
                    },
                rect.center().y,
            ),
            Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(if compact { 12.2 } else { 12.8 }),
            TEXT_PRIMARY,
        );
    }

    response
}

fn load_github_mark_texture(ctx: &Context) -> Option<egui::TextureHandle> {
    let image = image::load_from_memory_with_format(
        include_bytes!("../resourses/GitHubWhite20.png"),
        ImageFormat::Png,
    )
    .ok()?;
    let rgba = image.into_rgba8();
    let (width, height) = rgba.dimensions();
    let color_image =
        egui::ColorImage::from_rgba_unmultiplied([width as usize, height as usize], rgba.as_raw());

    Some(ctx.load_texture(
        "rgmr-github-mark-official",
        color_image,
        egui::TextureOptions::LINEAR,
    ))
}

fn field_label(ui: &mut Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .size(11.5)
            .color(TEXT_SECONDARY)
            .strong(),
    );
}

fn result_card(ui: &mut Ui, label: &str, value: Option<&str>, empty_label: &str, accent: Color32) {
    UiFrame::none()
        .fill(BG_INPUT)
        .stroke(Stroke::new(1.0, accent.linear_multiply(0.8)))
        .rounding(Rounding::same(18.0))
        .inner_margin(Margin::same(16.0))
        .show(ui, |ui| {
            ui.label(
                RichText::new(label)
                    .size(11.5)
                    .color(TEXT_SECONDARY)
                    .strong(),
            );
            ui.add_space(6.0);
            ui.label(
                RichText::new(value.unwrap_or(empty_label))
                    .size(17.0)
                    .color(if value.is_some() {
                        TEXT_PRIMARY
                    } else {
                        TEXT_DIM
                    })
                    .strong(),
            );
        });
}

fn loading_strip(ui: &mut Ui, text: &str) {
    UiFrame::none()
        .fill(BG_INPUT)
        .stroke(Stroke::new(1.0, BORDER_STRONG))
        .rounding(Rounding::same(16.0))
        .inner_margin(Margin::symmetric(12.0, 10.0))
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.add(egui::Spinner::new().size(16.0).color(ACCENT_SECONDARY));
                ui.add_space(8.0);
                ui.label(RichText::new(text).size(12.5).color(TEXT_PRIMARY));
            });
        });
}

fn notice_strip(ui: &mut Ui, text: &str, tone: Color32) {
    UiFrame::none()
        .fill(BG_INPUT)
        .stroke(Stroke::new(1.0, tone.linear_multiply(0.85)))
        .rounding(Rounding::same(16.0))
        .inner_margin(Margin::symmetric(12.0, 10.0))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("●").size(10.8).color(tone));
                ui.add_space(6.0);
                ui.label(RichText::new(text).size(12.4).color(TEXT_PRIMARY));
            });
        });
}

fn error_card(ui: &mut Ui, title: &str, text: &str) {
    UiFrame::none()
        .fill(Color32::from_rgba_premultiplied(255, 120, 136, 20))
        .stroke(Stroke::new(1.0, ERROR))
        .rounding(Rounding::same(16.0))
        .inner_margin(Margin::same(12.0))
        .show(ui, |ui| {
            ui.label(RichText::new(title).size(12.0).color(ERROR).strong());
            ui.add_space(6.0);
            ui.label(RichText::new(text).size(12.5).color(TEXT_PRIMARY));
        });
}

fn status_badge(ui: &mut Ui, text: &str, tone: Color32) {
    UiFrame::none()
        .fill(tone.linear_multiply(0.12))
        .stroke(Stroke::new(1.0, tone.linear_multiply(0.8)))
        .rounding(Rounding::same(999.0))
        .inner_margin(Margin::symmetric(10.0, 5.0))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(10.8).color(tone).strong());
        });
}

fn small_hint(ui: &mut Ui, text: &str, color: Color32) {
    ui.label(RichText::new(text).size(11.8).color(color));
}

fn stable_hint(ui: &mut Ui, text: &str, color: Color32, height: f32) {
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), height),
        Layout::top_down(Align::Min),
        |ui| small_hint(ui, text, color),
    );
}

fn labeled_text_input(
    ui: &mut Ui,
    label: &str,
    hint: &str,
    value: &mut String,
    secret: bool,
    show_secret: &mut bool,
    toggle_labels: Option<(&str, &str)>,
) -> bool {
    field_label(ui, label);

    if let Some((show_label, hide_label)) = toggle_labels {
        let mut changed = false;
        ui.horizontal(|ui| {
            let field_width = (ui.available_width() - 86.0).max(120.0);
            let response = ui.add_sized(
                [field_width, 38.0],
                TextEdit::singleline(value)
                    .password(secret && !*show_secret)
                    .hint_text(hint),
            );
            changed = response.changed();
            if ui
                .add(action_button(
                    if *show_secret { hide_label } else { show_label },
                    ACCENT_MUTED,
                    false,
                ))
                .clicked()
            {
                *show_secret = !*show_secret;
            }
        });
        changed
    } else {
        ui.add_sized(
            [ui.available_width(), 38.0],
            TextEdit::singleline(value)
                .password(secret && !*show_secret)
                .hint_text(hint),
        )
        .changed()
    }
}

fn action_button(label: &str, accent: Color32, filled: bool) -> Button<'_> {
    Button::new(RichText::new(label).size(12.8).color(TEXT_PRIMARY).strong())
        .fill(if filled {
            accent.linear_multiply(0.22)
        } else {
            BG_INPUT
        })
        .stroke(Stroke::new(1.1, accent))
        .rounding(Rounding::same(CONTROL_RADIUS))
}

fn danger_button(label: &str) -> Button<'_> {
    Button::new(RichText::new(label).size(12.8).color(TEXT_PRIMARY).strong())
        .fill(Color32::from_rgba_premultiplied(255, 120, 136, 18))
        .stroke(Stroke::new(1.1, ERROR))
        .rounding(Rounding::same(CONTROL_RADIUS))
}

fn titlebar_button(label: &str, accent: Color32, filled: bool) -> Button<'_> {
    Button::new(RichText::new(label).size(14.0).color(TEXT_PRIMARY).strong())
        .fill(if filled {
            accent.linear_multiply(0.22)
        } else {
            BG_INPUT
        })
        .stroke(Stroke::new(1.0, accent.linear_multiply(0.85)))
        .rounding(Rounding::same(12.0))
        .min_size(Vec2::new(34.0, 30.0))
}

fn toast_color(tone: &ToastTone) -> Color32 {
    match tone {
        ToastTone::Accent => ACCENT_SECONDARY,
        ToastTone::Success => SUCCESS,
        ToastTone::Warning => WARNING,
        ToastTone::Danger => ERROR,
    }
}

fn request_phase_color(phase: &RequestPhase) -> Color32 {
    match phase {
        RequestPhase::Idle => TEXT_SECONDARY,
        RequestPhase::ImageReady => ACCENT_SECONDARY,
        RequestPhase::Preparing | RequestPhase::Requesting => ACCENT_PRIMARY,
        RequestPhase::ParseSuccess => SUCCESS,
        RequestPhase::ParsePartial => WARNING,
        RequestPhase::Failed => ERROR,
    }
}

fn model_state_color(state: &ModelCatalogState) -> Color32 {
    match state {
        ModelCatalogState::Idle => TEXT_SECONDARY,
        ModelCatalogState::Loading => ACCENT_SECONDARY,
        ModelCatalogState::Ready => SUCCESS,
        ModelCatalogState::Empty => WARNING,
        ModelCatalogState::Error(_) => ERROR,
    }
}

fn parse_status_color(status: &ParseStatus) -> Color32 {
    match status {
        ParseStatus::Strict => SUCCESS,
        ParseStatus::Partial => WARNING,
        ParseStatus::Fallback => ERROR,
    }
}

fn image_display_name(language: Language, image: &crate::domain::ImageAsset) -> String {
    image
        .original_name
        .clone()
        .unwrap_or_else(|| i18n::image_source_name(language, &image.source_kind))
}

fn image_summary_line(language: Language, image: &crate::domain::ImageAsset) -> String {
    format!(
        "{} · {} · {}",
        i18n::image_source_label(language, &image.source_kind),
        image.dimensions_label(),
        image.mime_type
    )
}

fn missing_image_message(language: Language) -> &'static str {
    match language {
        Language::ZhCn => "请先粘贴或拖拽一张图片。",
        Language::EnUs => "Paste or drag an image first.",
        Language::RuRu => "Сначала вставьте или перетащите изображение.",
    }
}

fn prompt_is_risky(prompt: &str) -> bool {
    let lower = prompt.to_lowercase();
    !(prompt.contains('-')
        && (prompt.contains("置信说明")
            || lower.contains("confidence note")
            || prompt.contains("Примечание уверенности")))
}

fn window_rounding() -> Rounding {
    Rounding {
        nw: WINDOW_RADIUS,
        ne: WINDOW_RADIUS,
        sw: WINDOW_RADIUS,
        se: WINDOW_RADIUS,
    }
}

fn top_rounding() -> Rounding {
    Rounding {
        nw: WINDOW_RADIUS,
        ne: WINDOW_RADIUS,
        sw: 16.0,
        se: 16.0,
    }
}

fn bottom_rounding() -> Rounding {
    Rounding {
        nw: 16.0,
        ne: 16.0,
        sw: WINDOW_RADIUS,
        se: WINDOW_RADIUS,
    }
}

fn window_shadow() -> Shadow {
    Shadow {
        offset: egui::vec2(0.0, 18.0),
        blur: 42.0,
        spread: 0.0,
        color: Color32::from_black_alpha(112),
    }
}
