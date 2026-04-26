pub use analysis::{AnalysisOutcome, AnalysisRawResponse, AnalysisRequest};
pub use config::{
    ApiConfig, AppConfig, ConfigValidation, DEFAULT_BASE_URL, DEFAULT_TIMEOUT_SECS, PromptProfile,
    SecurityConfig, UiConfig,
};
pub use error::AppError;
pub use image_asset::{ImageAsset, ImageSourceKind};
pub use model::ModelDescriptor;
pub use parsing::{ParseStatus, ParsedLocation};
pub use validation::ValidationIssue;

pub mod validation {
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum ValidationIssue {
        BaseUrlRequired,
        ApiKeyRequired,
        ModelRequired,
    }
}

pub mod config {
    use std::collections::BTreeSet;

    use serde::{Deserialize, Serialize};

    use crate::i18n::{Language, default_system_prompt};

    use super::ValidationIssue;

    pub const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
    pub const DEFAULT_TIMEOUT_SECS: u64 = 45;

    fn default_version() -> String {
        "1.0.0".to_owned()
    }

    fn default_base_url() -> String {
        DEFAULT_BASE_URL.to_owned()
    }

    fn default_timeout_secs() -> u64 {
        DEFAULT_TIMEOUT_SECS
    }

    fn default_output_format_version() -> String {
        "v1".to_owned()
    }

    fn default_api_key_storage() -> String {
        "plain_text".to_owned()
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    #[serde(default)]
    pub struct AppConfig {
        pub version: String,
        pub api: ApiConfig,
        pub prompt: PromptProfile,
        pub security: SecurityConfig,
        pub ui: UiConfig,
    }

    impl Default for AppConfig {
        fn default() -> Self {
            let ui = UiConfig::default();
            Self {
                version: default_version(),
                api: ApiConfig::default(),
                prompt: PromptProfile::default_for_language(ui.language),
                security: SecurityConfig::default(),
                ui,
            }
        }
    }

    impl AppConfig {
        pub fn repair_defaults(&mut self) {
            self.ui.repair_defaults();
            self.api.repair_defaults();
            self.prompt.repair_defaults(self.ui.language);
            self.security.repair_defaults();

            if self.version.trim() != default_version() {
                self.version = default_version();
            }
        }

        pub fn validate(&self) -> ConfigValidation {
            let mut validation = ConfigValidation::default();

            if self.api.base_url.trim().is_empty() {
                validation.base_url = Some(ValidationIssue::BaseUrlRequired);
            }
            if self.api.api_key.trim().is_empty() {
                validation.api_key = Some(ValidationIssue::ApiKeyRequired);
            }
            if self.api.model.trim().is_empty() {
                validation.model = Some(ValidationIssue::ModelRequired);
            }

            validation
        }

        pub fn apply_language(&mut self, language: Language) {
            if self.ui.language == language {
                return;
            }

            let should_sync_prompt = self.prompt.looks_like_default_prompt();
            self.ui.language = language;

            if should_sync_prompt {
                self.prompt.system_prompt = default_system_prompt(language).to_owned();
            }
        }
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    #[serde(default)]
    pub struct ApiConfig {
        pub base_url: String,
        pub api_key: String,
        pub model: String,
        pub timeout_sec: u64,
    }

    impl Default for ApiConfig {
        fn default() -> Self {
            Self {
                base_url: default_base_url(),
                api_key: String::new(),
                model: String::new(),
                timeout_sec: default_timeout_secs(),
            }
        }
    }

    impl ApiConfig {
        pub fn repair_defaults(&mut self) {
            if self.base_url.trim().is_empty() {
                self.base_url = default_base_url();
            }
            self.timeout_sec = self.timeout_sec.clamp(10, 180);
        }

        pub fn normalized_base_url(&self) -> String {
            self.base_url.trim().trim_end_matches('/').to_owned()
        }

        pub fn clamped_timeout(&self) -> u64 {
            self.timeout_sec.clamp(10, 180)
        }

        pub fn catalog_identity(&self) -> String {
            format!("{}|{}", self.normalized_base_url(), self.api_key.trim())
        }

        pub fn chat_completion_endpoints(&self) -> Vec<String> {
            openai_endpoint_candidates(&self.base_url, "chat/completions")
        }

        pub fn model_catalog_endpoints(&self) -> Vec<String> {
            openai_endpoint_candidates(&self.base_url, "models")
        }
    }

    pub fn openai_endpoint_candidates(base_url: &str, tail: &str) -> Vec<String> {
        let normalized = base_url.trim().trim_end_matches('/');
        let canonical_tail = tail.trim().trim_matches('/');
        if normalized.is_empty() || canonical_tail.is_empty() {
            return Vec::new();
        }

        let mut seeds = BTreeSet::new();
        seeds.insert(normalized.to_owned());

        let stripped = strip_known_openai_endpoint_tail(normalized);
        if stripped != normalized {
            seeds.insert(stripped.to_owned());
        }

        let mut candidates = BTreeSet::new();
        for seed in seeds {
            let seed = seed.trim_end_matches('/');
            if seed.is_empty() {
                continue;
            }

            if seed.ends_with(canonical_tail) {
                candidates.insert(seed.to_owned());
                continue;
            }

            candidates.insert(format!("{seed}/{canonical_tail}"));

            if seed.ends_with("/v1") {
                if let Some(root) = seed.strip_suffix("/v1") {
                    let root = root.trim_end_matches('/');
                    if !root.is_empty() {
                        candidates.insert(format!("{root}/{canonical_tail}"));
                    }
                }
            } else {
                candidates.insert(format!("{seed}/v1/{canonical_tail}"));
            }
        }

        candidates.into_iter().collect()
    }

    fn strip_known_openai_endpoint_tail(url: &str) -> &str {
        const KNOWN_TAILS: [&str; 5] = [
            "/chat/completions",
            "/responses",
            "/completions",
            "/models",
            "/embeddings",
        ];

        KNOWN_TAILS
            .iter()
            .find_map(|suffix| url.strip_suffix(suffix))
            .map(|value| value.trim_end_matches('/'))
            .filter(|value| !value.is_empty())
            .unwrap_or(url)
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    #[serde(default)]
    pub struct PromptProfile {
        pub system_prompt: String,
        pub allow_confidence_note: bool,
        pub output_format_version: String,
    }

    impl PromptProfile {
        pub fn default_for_language(language: Language) -> Self {
            Self {
                system_prompt: default_system_prompt(language).to_owned(),
                allow_confidence_note: true,
                output_format_version: default_output_format_version(),
            }
        }

        pub fn repair_defaults(&mut self, language: Language) {
            if self.system_prompt.trim().is_empty() {
                self.system_prompt = default_system_prompt(language).to_owned();
            }
            if self.output_format_version.trim().is_empty() {
                self.output_format_version = default_output_format_version();
            }
        }

        pub fn looks_like_default_prompt(&self) -> bool {
            Language::all()
                .iter()
                .copied()
                .any(|language| self.system_prompt.trim() == default_system_prompt(language))
        }
    }

    impl Default for PromptProfile {
        fn default() -> Self {
            Self::default_for_language(Language::default())
        }
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    #[serde(default)]
    pub struct SecurityConfig {
        pub api_key_storage: String,
        pub reserved_provider: String,
    }

    impl SecurityConfig {
        pub fn repair_defaults(&mut self) {
            if self.api_key_storage.trim().is_empty() {
                self.api_key_storage = default_api_key_storage();
            }
        }
    }

    impl Default for SecurityConfig {
        fn default() -> Self {
            Self {
                api_key_storage: default_api_key_storage(),
                reserved_provider: String::new(),
            }
        }
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    #[serde(default)]
    pub struct UiConfig {
        pub language: Language,
        pub show_manual_model_fallback: bool,
    }

    impl UiConfig {
        pub fn repair_defaults(&mut self) {}
    }

    impl Default for UiConfig {
        fn default() -> Self {
            Self {
                language: Language::default(),
                show_manual_model_fallback: false,
            }
        }
    }

    #[derive(Clone, Debug, Default)]
    pub struct ConfigValidation {
        pub base_url: Option<ValidationIssue>,
        pub api_key: Option<ValidationIssue>,
        pub model: Option<ValidationIssue>,
    }

    impl ConfigValidation {
        pub fn is_valid(&self) -> bool {
            self.base_url.is_none() && self.api_key.is_none() && self.model.is_none()
        }

        pub fn first_issue(&self) -> Option<&ValidationIssue> {
            self.base_url
                .as_ref()
                .or(self.api_key.as_ref())
                .or(self.model.as_ref())
        }
    }
}

pub mod analysis {
    use super::AppConfig;
    use super::parsing::ParsedLocation;
    use crate::i18n::default_user_prompt;

    #[derive(Clone, Debug)]
    pub struct AnalysisRequest {
        pub request_id: String,
        pub base_url: String,
        pub api_key: String,
        pub model: String,
        pub system_prompt: String,
        pub user_prompt: String,
        pub image_data_url: String,
        pub timeout_sec: u64,
    }

    impl AnalysisRequest {
        pub fn endpoint_candidates(&self) -> Vec<String> {
            super::config::openai_endpoint_candidates(&self.base_url, "chat/completions")
        }

        pub fn from_config(config: &AppConfig, image_data_url: String, request_id: String) -> Self {
            Self {
                request_id,
                base_url: config.api.normalized_base_url(),
                api_key: config.api.api_key.trim().to_owned(),
                model: config.api.model.trim().to_owned(),
                system_prompt: config.prompt.system_prompt.trim().to_owned(),
                user_prompt: default_user_prompt(config.ui.language).to_owned(),
                image_data_url,
                timeout_sec: config.api.clamped_timeout(),
            }
        }
    }

    #[derive(Clone, Debug, Default)]
    pub struct AnalysisRawResponse {
        pub request_id: String,
        pub raw_text: String,
        pub provider_response_json: Option<String>,
        pub latency_ms: u128,
        pub resolved_endpoint: Option<String>,
    }

    #[derive(Clone, Debug)]
    pub struct AnalysisOutcome {
        pub raw: AnalysisRawResponse,
        pub parsed: ParsedLocation,
    }
}

pub mod model {
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct ModelDescriptor {
        pub id: String,
        pub owned_by: Option<String>,
    }
}

pub mod image_asset {
    use eframe::egui;

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum ImageSourceKind {
        Clipboard,
        DragDrop,
        FilePicker,
    }

    #[derive(Clone, Debug)]
    pub struct ImageAsset {
        pub source_kind: ImageSourceKind,
        pub original_name: Option<String>,
        pub mime_type: String,
        pub width: u32,
        pub height: u32,
        pub preview_rgba: Vec<u8>,
        pub upload_bytes: Vec<u8>,
        pub data_url: String,
        pub sha256: String,
        pub acquired_at_epoch_ms: u128,
    }

    impl ImageAsset {
        pub fn color_image(&self) -> egui::ColorImage {
            egui::ColorImage::from_rgba_unmultiplied(
                [self.width as usize, self.height as usize],
                &self.preview_rgba,
            )
        }

        pub fn dimensions_label(&self) -> String {
            format!("{} × {}", self.width, self.height)
        }
    }
}

pub mod parsing {
    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub enum ParseStatus {
        Strict,
        Partial,
        #[default]
        Fallback,
    }

    #[derive(Clone, Debug, Default)]
    pub struct ParsedLocation {
        pub continent_country: Option<String>,
        pub domestic_region: Option<String>,
        pub city_region: Option<String>,
        pub place_detail: Option<String>,
        pub confidence_note: Option<String>,
        pub parse_status: ParseStatus,
        pub normalized_line: Option<String>,
    }

    impl ParsedLocation {
        pub fn has_any_segment(&self) -> bool {
            self.continent_country.is_some()
                || self.domestic_region.is_some()
                || self.city_region.is_some()
                || self.place_detail.is_some()
        }

        pub fn structured_line(&self) -> Option<String> {
            match (
                self.continent_country.as_deref(),
                self.domestic_region.as_deref(),
                self.city_region.as_deref(),
                self.place_detail.as_deref(),
            ) {
                (Some(a), Some(b), Some(c), Some(d)) => Some(format!("{a}-{b}-{c}-{d}")),
                _ => self.normalized_line.clone(),
            }
        }

        pub fn full_copy_text(&self, raw_output: &str, confidence_prefix: &str) -> String {
            let mut text = self
                .structured_line()
                .unwrap_or_else(|| raw_output.trim().to_owned());

            if let Some(confidence_note) = &self.confidence_note {
                if !confidence_note.trim().is_empty() {
                    if !text.is_empty() {
                        text.push('\n');
                    }
                    text.push_str(confidence_prefix);
                    text.push_str(": ");
                    text.push_str(confidence_note.trim());
                }
            }

            text
        }
    }
}

pub mod error {
    use thiserror::Error;

    #[derive(Debug, Error, Clone)]
    pub enum AppError {
        #[error("Unable to locate the Windows configuration directory")]
        ConfigDirectoryUnavailable,
        #[error("Configuration store error: {0}")]
        ConfigStore(String),
        #[error("No image available in clipboard")]
        ClipboardImageMissing,
        #[error("Clipboard access error: {0}")]
        ClipboardUnavailable(String),
        #[error("Unsupported image input: {0}")]
        UnsupportedImage(String),
        #[error("Image processing error: {0}")]
        ImageProcessing(String),
        #[error("Validation error: {0}")]
        Validation(String),
        #[error("Network request error: {0}")]
        Network(String),
        #[error("Authentication failed")]
        Authentication,
        #[error("Too many requests")]
        RateLimited,
        #[error("Service error: {0}")]
        Service(String),
        #[error("Response format error: {0}")]
        ResponseFormat(String),
    }
}
