use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use arboard::Clipboard;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use directories::BaseDirs;
use image::{DynamicImage, GenericImageView, ImageFormat, imageops::FilterType};
use reqwest::blocking::Client;
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::domain::{
    AnalysisOutcome, AnalysisRawResponse, AnalysisRequest, ApiConfig, AppConfig, AppError,
    ImageAsset, ImageSourceKind, ModelDescriptor, ParseStatus, ParsedLocation,
};

const APP_NAME: &str = "RGMR";
const MAX_IMAGE_EDGE: u32 = 1600;

pub struct ConfigStore {
    config_path: PathBuf,
}

impl ConfigStore {
    pub fn new() -> Result<Self, AppError> {
        let base_dirs = BaseDirs::new().ok_or(AppError::ConfigDirectoryUnavailable)?;
        Ok(Self {
            config_path: base_dirs.config_dir().join(APP_NAME).join("config.toml"),
        })
    }

    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    pub fn load(&self) -> Result<AppConfig, AppError> {
        if !self.config_path.exists() {
            let config = AppConfig::default();
            self.save(&config)?;
            return Ok(config);
        }

        let text = fs::read_to_string(&self.config_path)
            .map_err(|err| AppError::ConfigStore(err.to_string()))?;
        let mut config: AppConfig =
            toml::from_str(&text).map_err(|err| AppError::ConfigStore(err.to_string()))?;
        config.repair_defaults();
        Ok(config)
    }

    pub fn save(&self, config: &AppConfig) -> Result<(), AppError> {
        if let Some(parent) = self.config_path.parent() {
            fs::create_dir_all(parent).map_err(|err| AppError::ConfigStore(err.to_string()))?;
        }

        let serialized = toml::to_string_pretty(config)
            .map_err(|err| AppError::ConfigStore(err.to_string()))?;
        fs::write(&self.config_path, serialized)
            .map_err(|err| AppError::ConfigStore(err.to_string()))
    }
}

pub struct ImagePipelineService;

impl ImagePipelineService {
    pub fn from_clipboard() -> Result<ImageAsset, AppError> {
        let mut clipboard = Clipboard::new()
            .map_err(|err| AppError::ClipboardUnavailable(err.to_string()))?;
        let image = clipboard
            .get_image()
            .map_err(|_| AppError::ClipboardImageMissing)?;

        let width = image.width as u32;
        let height = image.height as u32;
        let rgba = image.bytes.into_owned();
        let buffer = image::RgbaImage::from_raw(width, height, rgba)
            .ok_or_else(|| AppError::ImageProcessing("Clipboard bitmap format is invalid".to_owned()))?;

        Self::from_dynamic_image(
            DynamicImage::ImageRgba8(buffer),
            ImageSourceKind::Clipboard,
            None,
            Some("image/png".to_owned()),
        )
    }

    pub fn from_file(path: &Path, source_kind: ImageSourceKind) -> Result<ImageAsset, AppError> {
        let bytes = fs::read(path).map_err(|err| AppError::UnsupportedImage(err.to_string()))?;
        let image = image::load_from_memory(&bytes)
            .map_err(|err| AppError::ImageProcessing(err.to_string()))?;
        let mime = guess_mime_type(path);
        let name = path.file_name().map(|name| name.to_string_lossy().to_string());

        Self::from_dynamic_image(image, source_kind, name, Some(mime))
    }

    fn from_dynamic_image(
        image: DynamicImage,
        source_kind: ImageSourceKind,
        original_name: Option<String>,
        mime_hint: Option<String>,
    ) -> Result<ImageAsset, AppError> {
        let resized = resize_if_needed(image);
        let rgba = resized.to_rgba8();
        let (width, height) = resized.dimensions();

        let mut encoded = Cursor::new(Vec::new());
        resized
            .write_to(&mut encoded, ImageFormat::Png)
            .map_err(|err| AppError::ImageProcessing(err.to_string()))?;
        let upload_bytes = encoded.into_inner();
        let mime_type = mime_hint.unwrap_or_else(|| "image/png".to_owned());
        let data_url = format!(
            "data:image/png;base64,{}",
            STANDARD.encode(&upload_bytes)
        );
        let sha256 = format!("{:x}", Sha256::digest(&upload_bytes));
        let acquired_at_epoch_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or_default();

        Ok(ImageAsset {
            source_kind,
            original_name,
            mime_type,
            width,
            height,
            preview_rgba: rgba.into_raw(),
            upload_bytes,
            data_url,
            sha256,
            acquired_at_epoch_ms,
        })
    }
}

pub struct ResultParser;

impl ResultParser {
    pub fn parse(raw_text: &str) -> ParsedLocation {
        let cleaned = raw_text.replace('\r', "");
        let non_empty_lines: Vec<&str> = cleaned
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect();

        if non_empty_lines.is_empty() {
            return ParsedLocation {
                confidence_note: None,
                parse_status: ParseStatus::Fallback,
                normalized_line: None,
                ..Default::default()
            };
        }

        let main_line = normalize_delimiters(non_empty_lines[0]);
        let confidence_note = non_empty_lines
            .iter()
            .skip(1)
            .find_map(|line| extract_confidence_note(line));

        let raw_segments: Vec<String> = main_line
            .split('-')
            .map(|segment| segment.trim().trim_matches('"').trim_matches('`').to_owned())
            .filter(|segment| !segment.is_empty())
            .collect();

        if raw_segments.len() == 4 {
            return ParsedLocation {
                continent_country: Some(raw_segments[0].clone()),
                domestic_region: Some(raw_segments[1].clone()),
                city_region: Some(raw_segments[2].clone()),
                place_detail: Some(raw_segments[3].clone()),
                confidence_note,
                parse_status: ParseStatus::Strict,
                normalized_line: Some(raw_segments.join("-")),
            };
        }

        if raw_segments.len() > 4 {
            let first_four = raw_segments[..4].to_vec();
            return ParsedLocation {
                continent_country: Some(first_four[0].clone()),
                domestic_region: Some(first_four[1].clone()),
                city_region: Some(first_four[2].clone()),
                place_detail: Some(first_four[3].clone()),
                confidence_note,
                parse_status: ParseStatus::Partial,
                normalized_line: Some(first_four.join("-")),
            };
        }

        let mut fallback = ParsedLocation {
            confidence_note,
            parse_status: ParseStatus::Fallback,
            normalized_line: Some(main_line.clone()),
            ..Default::default()
        };

        if let Some(value) = raw_segments.first() {
            fallback.continent_country = Some(value.clone());
        }
        if let Some(value) = raw_segments.get(1) {
            fallback.domestic_region = Some(value.clone());
        }
        if let Some(value) = raw_segments.get(2) {
            fallback.city_region = Some(value.clone());
        }
        if let Some(value) = raw_segments.get(3) {
            fallback.place_detail = Some(value.clone());
        }

        if main_line.contains('-') || main_line.contains('—') || main_line.contains('－') {
            fallback.parse_status = ParseStatus::Partial;
        }

        fallback
    }
}

pub struct VisionClient {
    client: Client,
}

impl VisionClient {
    pub fn new(timeout_secs: u64) -> Result<Self, AppError> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(timeout_secs.clamp(10, 180)))
            .build()
            .map_err(|err| AppError::Network(err.to_string()))?;
        Ok(Self { client })
    }

    pub fn analyze(&self, request: &AnalysisRequest) -> Result<AnalysisOutcome, AppError> {
        let started = Instant::now();
        let body = ChatCompletionRequest::from_request(request);
        let mut last_error = None;

        for endpoint in request.endpoint_candidates() {
            let response = match self
                .client
                .post(&endpoint)
                .bearer_auth(request.api_key.trim())
                .json(&body)
                .send()
            {
                Ok(response) => response,
                Err(err) => {
                    last_error = Some(AppError::Network(err.to_string()));
                    continue;
                }
            };

            let status = response.status();
            let response_text = response
                .text()
                .map_err(|err| AppError::ResponseFormat(err.to_string()))?;

            if status.as_u16() == 401 || status.as_u16() == 403 {
                return Err(AppError::Authentication);
            }
            if status.as_u16() == 429 {
                return Err(AppError::RateLimited);
            }
            if status.as_u16() == 404 || status.as_u16() == 405 {
                last_error = Some(AppError::Network(format!(
                    "HTTP {}: {}",
                    status.as_u16(),
                    response_text
                )));
                continue;
            }
            if status.is_server_error() {
                return Err(AppError::Service(response_text));
            }
            if !status.is_success() {
                return Err(AppError::Network(format!(
                    "HTTP {}: {}",
                    status.as_u16(),
                    response_text
                )));
            }

            let value: Value = serde_json::from_str(&response_text)
                .map_err(|err| AppError::ResponseFormat(err.to_string()))?;
            let raw_text = extract_text_from_response(&value)?;
            let parsed = ResultParser::parse(&raw_text);
            let raw = AnalysisRawResponse {
                request_id: request.request_id.clone(),
                raw_text,
                provider_response_json: Some(response_text),
                latency_ms: started.elapsed().as_millis(),
                resolved_endpoint: Some(endpoint),
            };

            return Ok(AnalysisOutcome { raw, parsed });
        }

        Err(last_error.unwrap_or_else(|| {
            AppError::Network(
                "No reachable OpenAI-compatible chat completion endpoint was found".to_owned(),
            )
        }))
    }

    pub fn fetch_models(&self, api: &ApiConfig) -> Result<Vec<ModelDescriptor>, AppError> {
        let mut last_error = None;

        for endpoint in api.model_catalog_endpoints() {
            let response = match self
                .client
                .get(&endpoint)
                .bearer_auth(api.api_key.trim())
                .send()
            {
                Ok(response) => response,
                Err(err) => {
                    last_error = Some(AppError::Network(err.to_string()));
                    continue;
                }
            };

            let status = response.status();
            let response_text = response
                .text()
                .map_err(|err| AppError::ResponseFormat(err.to_string()))?;

            if status.as_u16() == 401 || status.as_u16() == 403 {
                return Err(AppError::Authentication);
            }
            if status.as_u16() == 429 {
                return Err(AppError::RateLimited);
            }
            if status.as_u16() == 404 || status.as_u16() == 405 {
                last_error = Some(AppError::Network(format!(
                    "HTTP {}: {}",
                    status.as_u16(),
                    response_text
                )));
                continue;
            }
            if status.is_server_error() {
                return Err(AppError::Service(response_text));
            }
            if !status.is_success() {
                return Err(AppError::Network(format!(
                    "HTTP {}: {}",
                    status.as_u16(),
                    response_text
                )));
            }

            let value: Value = serde_json::from_str(&response_text)
                .map_err(|err| AppError::ResponseFormat(err.to_string()))?;
            return extract_models_from_response(&value);
        }

        Err(last_error.unwrap_or_else(|| {
            AppError::Network("No reachable OpenAI-compatible model catalog endpoint was found".to_owned())
        }))
    }
}

pub fn build_analysis_request(config: &AppConfig, image: &ImageAsset) -> AnalysisRequest {
    AnalysisRequest::from_config(config, image.data_url.clone(), Uuid::new_v4().to_string())
}

fn resize_if_needed(image: DynamicImage) -> DynamicImage {
    let (width, height) = image.dimensions();
    let max_edge = width.max(height);
    if max_edge <= MAX_IMAGE_EDGE {
        return image;
    }

    let scale = MAX_IMAGE_EDGE as f32 / max_edge as f32;
    let new_width = ((width as f32) * scale).round().max(1.0) as u32;
    let new_height = ((height as f32) * scale).round().max(1.0) as u32;
    image.resize(new_width, new_height, FilterType::Lanczos3)
}

fn guess_mime_type(path: &Path) -> String {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        _ => "image/png",
    }
    .to_owned()
}

fn normalize_delimiters(input: &str) -> String {
    input
        .trim()
        .replace('—', "-")
        .replace('–', "-")
        .replace('－', "-")
        .replace('﹣', "-")
        .replace(" - ", "-")
}

fn extract_confidence_note(line: &str) -> Option<String> {
    let trimmed = line.trim();
    [
        "置信说明:",
        "置信说明：",
        "Confidence note:",
        "Confidence note：",
        "Примечание уверенности:",
        "Примечание уверенности：",
    ]
    .iter()
    .find_map(|prefix| trimmed.strip_prefix(prefix))
    .map(|value| value.trim().chars().take(64).collect::<String>())
    .filter(|value| !value.is_empty())
}

fn extract_text_from_response(value: &Value) -> Result<String, AppError> {
    let content = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .ok_or_else(|| AppError::ResponseFormat("missing choices[0].message.content".to_owned()))?;

    if let Some(text) = content.as_str() {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Err(AppError::ResponseFormat("model returned empty text".to_owned()));
        }
        return Ok(trimmed.to_owned());
    }

    if let Some(array) = content.as_array() {
        let merged = array
            .iter()
            .filter_map(|item| {
                item.get("text")
                    .and_then(Value::as_str)
                    .or_else(|| item.get("content").and_then(Value::as_str))
            })
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n");

        if merged.trim().is_empty() {
            return Err(AppError::ResponseFormat(
                "model returned a content array without text fragments".to_owned(),
            ));
        }

        return Ok(merged);
    }

    Err(AppError::ResponseFormat(
        "model returned an unsupported content structure".to_owned(),
    ))
}

fn extract_models_from_response(value: &Value) -> Result<Vec<ModelDescriptor>, AppError> {
    let models = value
        .get("data")
        .and_then(Value::as_array)
        .or_else(|| value.get("models").and_then(Value::as_array))
        .or_else(|| value.as_array())
        .ok_or_else(|| AppError::ResponseFormat("missing model array".to_owned()))?;

    let mut result = models
        .iter()
        .filter_map(|item| {
            let id = item
                .get("id")
                .or_else(|| item.get("name"))
                .and_then(Value::as_str)?
                .trim()
                .to_owned();
            if id.is_empty() {
                return None;
            }

            let owned_by = item
                .get("owned_by")
                .or_else(|| item.get("provider"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);

            Some(ModelDescriptor { id, owned_by })
        })
        .collect::<Vec<_>>();

    result.sort_by_key(|item| item.id.to_ascii_lowercase());
    result.dedup_by(|a, b| a.id == b.id);
    Ok(result)
}

#[derive(Serialize)]
struct ChatCompletionRequest {
    model: String,
    temperature: f32,
    max_tokens: u32,
    messages: Vec<ChatMessage>,
}

impl ChatCompletionRequest {
    fn from_request(request: &AnalysisRequest) -> Self {
        Self {
            model: request.model.clone(),
            temperature: 0.2,
            max_tokens: 160,
            messages: vec![
                ChatMessage::system(request.system_prompt.clone()),
                ChatMessage::user_multimodal(
                    request.user_prompt.clone(),
                    request.image_data_url.clone(),
                ),
            ],
        }
    }
}

#[derive(Serialize)]
struct ChatMessage {
    role: String,
    content: Value,
}

impl ChatMessage {
    fn system(text: String) -> Self {
        Self {
            role: "system".to_owned(),
            content: Value::String(text),
        }
    }

    fn user_multimodal(text: String, image_data_url: String) -> Self {
        Self {
            role: "user".to_owned(),
            content: json!([
                {
                    "type": "text",
                    "text": text
                },
                {
                    "type": "image_url",
                    "image_url": {
                        "url": image_data_url
                    }
                }
            ]),
        }
    }
}
