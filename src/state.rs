use std::time::{Duration, Instant};

use eframe::egui;

use crate::domain::{
    AnalysisOutcome, AppConfig, AppError, ImageAsset, ModelDescriptor, ParseStatus, ParsedLocation,
    ValidationIssue,
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum RequestPhase {
    #[default]
    Idle,
    ImageReady,
    Preparing,
    Requesting,
    ParseSuccess,
    ParsePartial,
    Failed,
}

impl RequestPhase {
    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Preparing | Self::Requesting)
    }
}

#[derive(Clone, Debug)]
pub enum SaveState {
    Idle,
    Saving,
    Saved,
    Invalid(ValidationIssue),
    Error(AppError),
}

impl Default for SaveState {
    fn default() -> Self {
        Self::Idle
    }
}

#[derive(Clone, Debug)]
pub enum ModelCatalogState {
    Idle,
    Loading,
    Ready,
    Empty,
    Error(AppError),
}

impl Default for ModelCatalogState {
    fn default() -> Self {
        Self::Idle
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToastTone {
    Accent,
    Success,
    Warning,
    Danger,
}

#[derive(Clone, Debug)]
pub struct Toast {
    pub tone: ToastTone,
    pub message: String,
    pub created_at: Instant,
    pub duration: Duration,
}

impl Toast {
    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed() >= self.duration
    }
}

pub struct LoadedImageState {
    pub asset: ImageAsset,
    pub texture: Option<egui::TextureHandle>,
}

impl LoadedImageState {
    pub fn ensure_texture(&mut self, ctx: &egui::Context) -> &egui::TextureHandle {
        if self.texture.is_none() {
            let texture = ctx.load_texture(
                format!("rgmr-image-{}", self.asset.sha256),
                self.asset.color_image(),
                egui::TextureOptions::LINEAR,
            );
            self.texture = Some(texture);
        }

        self.texture
            .as_ref()
            .expect("texture handle must exist after loading")
    }
}

pub struct AppState {
    pub config: AppConfig,
    pub image: Option<LoadedImageState>,
    pub request_phase: RequestPhase,
    pub save_state: SaveState,
    pub parsed_result: Option<ParsedLocation>,
    pub raw_output: String,
    pub error: Option<AppError>,
    pub last_request_latency_ms: Option<u128>,
    pub show_api_key: bool,
    pub dirty_config: bool,
    pub last_config_edit_at: Option<Instant>,
    pub toasts: Vec<Toast>,
    pub model_catalog: Vec<ModelDescriptor>,
    pub model_catalog_state: ModelCatalogState,
    pub model_catalog_source_identity: Option<String>,
}

impl AppState {
    pub fn new(mut config: AppConfig) -> Self {
        config.repair_defaults();

        Self {
            config,
            image: None,
            request_phase: RequestPhase::Idle,
            save_state: SaveState::Idle,
            parsed_result: None,
            raw_output: String::new(),
            error: None,
            last_request_latency_ms: None,
            show_api_key: false,
            dirty_config: false,
            last_config_edit_at: None,
            toasts: Vec::new(),
            model_catalog: Vec::new(),
            model_catalog_state: ModelCatalogState::Idle,
            model_catalog_source_identity: None,
        }
    }

    pub fn mark_config_dirty(&mut self) {
        self.dirty_config = true;
        self.last_config_edit_at = Some(Instant::now());
        self.save_state = SaveState::Saving;
    }

    pub fn save_success(&mut self) {
        self.dirty_config = false;
        self.save_state = SaveState::Saved;
    }

    pub fn save_invalid(&mut self, issue: &ValidationIssue) {
        self.dirty_config = false;
        self.save_state = SaveState::Invalid(issue.clone());
    }

    pub fn save_error(&mut self, error: AppError) {
        self.dirty_config = false;
        self.save_state = SaveState::Error(error);
    }

    pub fn set_image(&mut self, asset: ImageAsset) {
        self.image = Some(LoadedImageState {
            asset,
            texture: None,
        });
        self.request_phase = RequestPhase::ImageReady;
        self.error = None;
    }

    pub fn clear_image(&mut self) {
        self.image = None;
        self.request_phase = RequestPhase::Idle;
        self.error = None;
    }

    pub fn set_error(&mut self, error: AppError) {
        self.error = Some(error);
        self.request_phase = RequestPhase::Failed;
    }

    pub fn clear_error(&mut self) {
        self.error = None;
    }

    pub fn apply_analysis_outcome(&mut self, outcome: AnalysisOutcome) {
        self.raw_output = outcome.raw.raw_text.clone();
        self.last_request_latency_ms = Some(outcome.raw.latency_ms);
        self.error = None;
        self.parsed_result = Some(outcome.parsed.clone());
        self.request_phase = match outcome.parsed.parse_status {
            ParseStatus::Strict => RequestPhase::ParseSuccess,
            ParseStatus::Partial | ParseStatus::Fallback => RequestPhase::ParsePartial,
        };
    }

    pub fn mark_model_catalog_loading(&mut self) {
        self.model_catalog_state = ModelCatalogState::Loading;
        self.error = None;
    }

    pub fn apply_model_catalog(&mut self, identity: String, models: Vec<ModelDescriptor>) -> bool {
        self.model_catalog = models;
        self.model_catalog_source_identity = Some(identity);
        self.model_catalog_state = if self.model_catalog.is_empty() {
            ModelCatalogState::Empty
        } else {
            ModelCatalogState::Ready
        };

        let current = self.config.api.model.trim();
        let keep_current = !current.is_empty() && self.model_catalog.iter().any(|model| model.id == current);
        if keep_current {
            return false;
        }

        if let Some(first) = self.model_catalog.first() {
            self.config.api.model = first.id.clone();
            self.mark_config_dirty();
            return true;
        }

        false
    }

    pub fn set_model_catalog_error(&mut self, error: AppError) {
        self.model_catalog_state = ModelCatalogState::Error(error.clone());
        self.error = Some(error);
        self.model_catalog_source_identity = None;
    }

    pub fn mark_model_catalog_stale(&mut self) {
        self.model_catalog_state = ModelCatalogState::Idle;
        self.model_catalog_source_identity = None;
    }

    pub fn model_catalog_matches_current_api(&self) -> bool {
        self.model_catalog_source_identity
            .as_deref()
            .map(|identity| identity == self.config.api.catalog_identity())
            .unwrap_or(false)
    }

    pub fn push_toast(&mut self, tone: ToastTone, message: impl Into<String>) {
        self.toasts.push(Toast {
            tone,
            message: message.into(),
            created_at: Instant::now(),
            duration: Duration::from_millis(2600),
        });
    }

    pub fn prune_toasts(&mut self) {
        self.toasts.retain(|toast| !toast.is_expired());
    }
}
