use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use arboard::Clipboard;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use directories::BaseDirs;
use image::{
    DynamicImage, GenericImageView, ImageFormat, codecs::jpeg::JpegEncoder, imageops::FilterType,
};
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
const CLIPBOARD_MAX_WIDTH: u32 = 1080;
const CLIPBOARD_JPEG_QUALITY: u8 = 88;
const RGMR_USER_AGENT: &str = concat!("RGMR/", env!("CARGO_PKG_VERSION"));
const MAX_RESPONSE_PREVIEW_CHARS: usize = 220;
const MAX_DIAGNOSTIC_ITEMS: usize = 4;
const VISION_ROUTE_STAGGERS_MS: [u64; 3] = [0, 180, 360];

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

        let serialized =
            toml::to_string_pretty(config).map_err(|err| AppError::ConfigStore(err.to_string()))?;
        fs::write(&self.config_path, serialized)
            .map_err(|err| AppError::ConfigStore(err.to_string()))
    }
}

pub struct ImagePipelineService;

impl ImagePipelineService {
    pub fn from_clipboard() -> Result<ImageAsset, AppError> {
        let mut clipboard =
            Clipboard::new().map_err(|err| AppError::ClipboardUnavailable(err.to_string()))?;
        let image = clipboard
            .get_image()
            .map_err(|_| AppError::ClipboardImageMissing)?;

        let width = image.width as u32;
        let height = image.height as u32;
        let rgba = image.bytes.into_owned();
        let buffer = image::RgbaImage::from_raw(width, height, rgba).ok_or_else(|| {
            AppError::ImageProcessing("Clipboard bitmap format is invalid".to_owned())
        })?;

        Self::from_clipboard_image(DynamicImage::ImageRgba8(buffer))
    }

    pub fn from_file(path: &Path, source_kind: ImageSourceKind) -> Result<ImageAsset, AppError> {
        let bytes = fs::read(path).map_err(|err| AppError::UnsupportedImage(err.to_string()))?;
        let image = image::load_from_memory(&bytes)
            .map_err(|err| AppError::ImageProcessing(err.to_string()))?;
        let mime = guess_mime_type(path);
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string());

        Self::from_dynamic_image(image, source_kind, name, Some(mime))
    }

    fn from_clipboard_image(image: DynamicImage) -> Result<ImageAsset, AppError> {
        let resized = resize_clipboard_image(image);
        let rgba = resized.to_rgba8();
        let (width, height) = resized.dimensions();
        let rgb = flatten_rgba_over_white(&rgba);

        let mut encoded = Cursor::new(Vec::new());
        let mut encoder = JpegEncoder::new_with_quality(&mut encoded, CLIPBOARD_JPEG_QUALITY);
        encoder
            .encode_image(&DynamicImage::ImageRgb8(rgb))
            .map_err(|err| AppError::ImageProcessing(err.to_string()))?;
        let upload_bytes = encoded.into_inner();
        let mime_type = "image/jpeg".to_owned();
        let data_url = format!(
            "data:{};base64,{}",
            mime_type,
            STANDARD.encode(&upload_bytes)
        );
        let sha256 = format!("{:x}", Sha256::digest(&upload_bytes));
        let acquired_at_epoch_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or_default();

        Ok(ImageAsset {
            source_kind: ImageSourceKind::Clipboard,
            original_name: None,
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
        let data_url = format!("data:image/png;base64,{}", STANDARD.encode(&upload_bytes));
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
            .map(|segment| {
                segment
                    .trim()
                    .trim_matches('"')
                    .trim_matches('`')
                    .to_owned()
            })
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

#[derive(Clone)]
pub struct VisionClient {
    client: Client,
}

impl VisionClient {
    pub fn new(timeout_secs: u64) -> Result<Self, AppError> {
        let clamped_timeout = timeout_secs.clamp(10, 180);
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(clamped_timeout.min(20)))
            .timeout(Duration::from_secs(clamped_timeout))
            .build()
            .map_err(|err| AppError::Network(err.to_string()))?;
        Ok(Self { client })
    }

    pub fn analyze(&self, request: &AnalysisRequest) -> Result<AnalysisOutcome, AppError> {
        let endpoints = request.endpoint_candidates();
        if endpoints.is_empty() {
            return Err(AppError::Validation(
                "Base URL did not produce any OpenAI-compatible chat completion endpoint candidate"
                    .to_owned(),
            ));
        }

        let started = Instant::now();
        let body = ChatCompletionRequest::from_request(request);
        let completed = Arc::new(AtomicBool::new(false));
        let route_count = VISION_ROUTE_STAGGERS_MS.len();
        let (tx, rx) = mpsc::channel();

        for (route_index, stagger_ms) in VISION_ROUTE_STAGGERS_MS.iter().copied().enumerate() {
            let tx = tx.clone();
            let client = self.clone();
            let request = request.clone();
            let body = body.clone();
            let endpoints = rotate_candidates(&endpoints, route_index);
            let completed = Arc::clone(&completed);

            thread::spawn(move || {
                if stagger_ms > 0 {
                    thread::sleep(Duration::from_millis(stagger_ms));
                    if completed.load(Ordering::Acquire) {
                        return;
                    }
                }

                let result = analyze_route(
                    &client,
                    &request,
                    &body,
                    &endpoints,
                    started,
                    &completed,
                    route_index,
                );

                if completed.load(Ordering::Acquire) && result.is_err() {
                    return;
                }

                let _ = tx.send(result);
            });
        }
        drop(tx);

        let mut errors = Vec::new();
        while let Ok(result) = rx.recv() {
            match result {
                Ok(outcome) => {
                    completed.store(true, Ordering::Release);
                    return Ok(outcome);
                }
                Err(err) => {
                    errors.push(err);
                    if errors.len() == route_count {
                        break;
                    }
                }
            }
        }

        Err(combine_parallel_route_errors(
            "vision analysis",
            &errors,
            route_count,
        ))
    }

    pub fn fetch_models(&self, api: &ApiConfig) -> Result<Vec<ModelDescriptor>, AppError> {
        let endpoints = api.model_catalog_endpoints();
        if endpoints.is_empty() {
            return Err(AppError::Validation(
                "Base URL did not produce any OpenAI-compatible model catalog endpoint candidate"
                    .to_owned(),
            ));
        }

        let mut errors = Vec::new();
        for endpoint in endpoints {
            match fetch_models_from_endpoint(self, api, &endpoint) {
                Ok(models) => return Ok(models),
                Err(err) => errors.push((endpoint, err)),
            }
        }

        Err(combine_attempt_errors("model catalog request", &errors))
    }

    fn authorized_get(&self, endpoint: &str, api_key: &str) -> reqwest::blocking::RequestBuilder {
        let api_key = api_key.trim();
        self.client
            .get(endpoint)
            .bearer_auth(api_key)
            .header("Accept", "application/json, text/plain, */*")
            .header("Content-Type", "application/json")
            .header("User-Agent", RGMR_USER_AGENT)
            .header("api-key", api_key)
            .header("x-api-key", api_key)
    }

    fn authorized_post_json<T: Serialize + ?Sized>(
        &self,
        endpoint: &str,
        api_key: &str,
        body: &T,
    ) -> reqwest::blocking::RequestBuilder {
        let api_key = api_key.trim();
        self.client
            .post(endpoint)
            .bearer_auth(api_key)
            .header("Accept", "application/json, text/plain, */*")
            .header("Content-Type", "application/json")
            .header("User-Agent", RGMR_USER_AGENT)
            .header("api-key", api_key)
            .header("x-api-key", api_key)
            .json(body)
    }
}

pub fn build_analysis_request(config: &AppConfig, image: &ImageAsset) -> AnalysisRequest {
    AnalysisRequest::from_config(config, image.data_url.clone(), Uuid::new_v4().to_string())
}

fn analyze_route(
    client: &VisionClient,
    request: &AnalysisRequest,
    body: &ChatCompletionRequest,
    endpoints: &[String],
    started: Instant,
    completed: &AtomicBool,
    route_index: usize,
) -> Result<AnalysisOutcome, AppError> {
    let mut errors = Vec::new();

    for endpoint in endpoints {
        if completed.load(Ordering::Acquire) {
            break;
        }

        match analyze_endpoint(client, request, body, endpoint, started) {
            Ok(outcome) => return Ok(outcome),
            Err(err) => errors.push((endpoint.clone(), err)),
        }
    }

    Err(combine_attempt_errors(
        &format!("vision route {}", route_index + 1),
        &errors,
    ))
}

fn analyze_endpoint(
    client: &VisionClient,
    request: &AnalysisRequest,
    body: &ChatCompletionRequest,
    endpoint: &str,
    started: Instant,
) -> Result<AnalysisOutcome, AppError> {
    let response = client
        .authorized_post_json(endpoint, &request.api_key, body)
        .send()
        .map_err(|err| AppError::Network(err.to_string()))?;

    let status = response.status();
    let response_text = response
        .text()
        .map_err(|err| AppError::ResponseFormat(err.to_string()))?;

    validate_response_status(status.as_u16(), status.is_server_error(), &response_text)?;

    let value = parse_response_json(&response_text)?;
    let raw_text = extract_text_from_response(&value)?;
    let parsed = ResultParser::parse(&raw_text);
    let raw = AnalysisRawResponse {
        request_id: request.request_id.clone(),
        raw_text,
        provider_response_json: Some(response_text),
        latency_ms: started.elapsed().as_millis(),
        resolved_endpoint: Some(endpoint.to_owned()),
    };

    Ok(AnalysisOutcome { raw, parsed })
}

fn fetch_models_from_endpoint(
    client: &VisionClient,
    api: &ApiConfig,
    endpoint: &str,
) -> Result<Vec<ModelDescriptor>, AppError> {
    let response = client
        .authorized_get(endpoint, &api.api_key)
        .send()
        .map_err(|err| AppError::Network(err.to_string()))?;

    let status = response.status();
    let response_text = response
        .text()
        .map_err(|err| AppError::ResponseFormat(err.to_string()))?;

    validate_response_status(status.as_u16(), status.is_server_error(), &response_text)?;

    let value = parse_response_json(&response_text)?;
    extract_models_from_response(&value)
}

fn validate_response_status(
    status_code: u16,
    is_server_error: bool,
    response_text: &str,
) -> Result<(), AppError> {
    if status_code == 401 || status_code == 403 {
        return Err(AppError::Authentication);
    }
    if status_code == 429 {
        return Err(AppError::RateLimited);
    }
    if (200..=299).contains(&status_code) {
        return Ok(());
    }

    let preview = response_preview(response_text, MAX_RESPONSE_PREVIEW_CHARS);
    let message = if preview.is_empty() {
        format!("HTTP {status_code}")
    } else {
        format!("HTTP {status_code}: {preview}")
    };

    if is_server_error {
        Err(AppError::Service(message))
    } else {
        Err(AppError::Network(message))
    }
}

fn parse_response_json(response_text: &str) -> Result<Value, AppError> {
    let payload = extract_json_payload(response_text)?;
    serde_json::from_str(&payload).map_err(|err| {
        AppError::ResponseFormat(format!(
            "invalid JSON after preprocessing: {err}; preview: {}",
            response_preview(&payload, MAX_RESPONSE_PREVIEW_CHARS)
        ))
    })
}

fn extract_json_payload(response_text: &str) -> Result<String, AppError> {
    let trimmed = response_text.trim_start_matches('\u{feff}').trim();
    if trimmed.is_empty() {
        return Err(AppError::ResponseFormat(
            "response body was empty".to_owned(),
        ));
    }

    if let Some(fenced) = extract_code_fence_content(trimmed) {
        return extract_json_payload(&fenced);
    }

    if trimmed.starts_with("data:") {
        let merged = trimmed
            .lines()
            .filter_map(|line| line.trim().strip_prefix("data:"))
            .map(str::trim)
            .filter(|line| !line.is_empty() && *line != "[DONE]")
            .collect::<Vec<_>>()
            .join("\n");

        if !merged.is_empty() {
            return extract_json_payload(&merged);
        }
    }

    if trimmed.starts_with('<') {
        return Err(AppError::ResponseFormat(format!(
            "received HTML or proxy content instead of JSON: {}",
            response_preview(trimmed, MAX_RESPONSE_PREVIEW_CHARS)
        )));
    }

    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return Ok(trimmed.to_owned());
    }

    if let Some(candidate) = extract_first_json_value(trimmed) {
        return Ok(candidate.trim().to_owned());
    }

    Err(AppError::ResponseFormat(format!(
        "response did not contain a JSON document: {}",
        response_preview(trimmed, MAX_RESPONSE_PREVIEW_CHARS)
    )))
}

fn extract_code_fence_content(text: &str) -> Option<String> {
    if !text.starts_with("```") {
        return None;
    }

    let after_ticks = text.strip_prefix("```")?;
    let body = after_ticks
        .split_once('\n')
        .map(|(_, body)| body)
        .unwrap_or(after_ticks);
    let end = body.rfind("```")?;
    Some(body[..end].trim().to_owned())
}

fn extract_first_json_value(text: &str) -> Option<&str> {
    let mut start_index = None;
    let mut stack = Vec::new();
    let mut in_string = false;
    let mut escaped = false;

    for (index, ch) in text.char_indices() {
        if start_index.is_none() {
            match ch {
                '{' => {
                    start_index = Some(index);
                    stack.push('}');
                }
                '[' => {
                    start_index = Some(index);
                    stack.push(']');
                }
                _ => {}
            }
            continue;
        }

        if in_string {
            if escaped {
                escaped = false;
                continue;
            }

            match ch {
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }

        match ch {
            '"' => in_string = true,
            '{' => stack.push('}'),
            '[' => stack.push(']'),
            '}' | ']' => {
                if stack.pop() != Some(ch) {
                    return None;
                }
                if stack.is_empty() {
                    let start = start_index?;
                    return Some(&text[start..index + ch.len_utf8()]);
                }
            }
            _ => {}
        }
    }

    None
}

fn response_preview(text: &str, limit: usize) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut preview = collapsed.chars().take(limit).collect::<String>();
    if collapsed.chars().count() > limit {
        preview.push('…');
    }
    preview
}

fn preview_json(value: &Value) -> String {
    serde_json::to_string(value)
        .map(|json| response_preview(&json, MAX_RESPONSE_PREVIEW_CHARS))
        .unwrap_or_else(|_| "<unserializable json>".to_owned())
}

fn rotate_candidates(candidates: &[String], offset: usize) -> Vec<String> {
    if candidates.is_empty() {
        return Vec::new();
    }

    let len = candidates.len();
    candidates
        .iter()
        .cycle()
        .skip(offset % len)
        .take(len)
        .cloned()
        .collect()
}

fn combine_attempt_errors(context: &str, errors: &[(String, AppError)]) -> AppError {
    if errors
        .iter()
        .any(|(_, err)| matches!(err, AppError::Authentication))
    {
        return AppError::Authentication;
    }
    if errors
        .iter()
        .any(|(_, err)| matches!(err, AppError::RateLimited))
    {
        return AppError::RateLimited;
    }
    if errors.is_empty() {
        return AppError::Network(format!(
            "{context} failed without receiving any provider response"
        ));
    }

    let details = errors
        .iter()
        .take(MAX_DIAGNOSTIC_ITEMS)
        .map(|(endpoint, err)| format!("{endpoint} -> {}", concise_error(err)))
        .collect::<Vec<_>>();
    let suffix = if errors.len() > MAX_DIAGNOSTIC_ITEMS {
        format!(
            " | +{} more candidate(s)",
            errors.len() - MAX_DIAGNOSTIC_ITEMS
        )
    } else {
        String::new()
    };
    let message = format!(
        "{context} failed across {} endpoint candidate(s): {}{}",
        errors.len(),
        details.join(" | "),
        suffix
    );

    if errors
        .iter()
        .all(|(_, err)| matches!(err, AppError::ResponseFormat(_)))
    {
        AppError::ResponseFormat(message)
    } else if errors
        .iter()
        .any(|(_, err)| matches!(err, AppError::Service(_)))
    {
        AppError::Service(message)
    } else {
        AppError::Network(message)
    }
}

fn combine_parallel_route_errors(
    context: &str,
    errors: &[AppError],
    route_count: usize,
) -> AppError {
    if errors
        .iter()
        .any(|err| matches!(err, AppError::Authentication))
    {
        return AppError::Authentication;
    }
    if errors
        .iter()
        .any(|err| matches!(err, AppError::RateLimited))
    {
        return AppError::RateLimited;
    }
    if errors.is_empty() {
        return AppError::Network(format!(
            "All {route_count} concurrent {context} routes ended without a usable response"
        ));
    }

    let details = errors
        .iter()
        .take(MAX_DIAGNOSTIC_ITEMS)
        .map(concise_error)
        .collect::<Vec<_>>();
    let suffix = if errors.len() > MAX_DIAGNOSTIC_ITEMS {
        format!(" | +{} more route(s)", errors.len() - MAX_DIAGNOSTIC_ITEMS)
    } else {
        String::new()
    };
    let message = format!(
        "All {route_count} concurrent {context} routes failed: {}{}",
        details.join(" | "),
        suffix
    );

    if errors
        .iter()
        .all(|err| matches!(err, AppError::ResponseFormat(_)))
    {
        AppError::ResponseFormat(message)
    } else if errors.iter().any(|err| matches!(err, AppError::Service(_))) {
        AppError::Service(message)
    } else {
        AppError::Network(message)
    }
}

fn concise_error(error: &AppError) -> String {
    match error {
        AppError::ConfigStore(message)
        | AppError::ClipboardUnavailable(message)
        | AppError::UnsupportedImage(message)
        | AppError::ImageProcessing(message)
        | AppError::Validation(message)
        | AppError::Network(message)
        | AppError::Service(message)
        | AppError::ResponseFormat(message) => message.clone(),
        AppError::ConfigDirectoryUnavailable
        | AppError::ClipboardImageMissing
        | AppError::Authentication
        | AppError::RateLimited => error.to_string(),
    }
}

fn resize_clipboard_image(image: DynamicImage) -> DynamicImage {
    let (width, height) = image.dimensions();
    if width <= CLIPBOARD_MAX_WIDTH {
        return image;
    }

    let scale = CLIPBOARD_MAX_WIDTH as f32 / width as f32;
    let new_height = ((height as f32) * scale).round().max(1.0) as u32;
    image.resize(CLIPBOARD_MAX_WIDTH, new_height, FilterType::Lanczos3)
}

fn flatten_rgba_over_white(image: &image::RgbaImage) -> image::RgbImage {
    let (width, height) = image.dimensions();
    let mut flattened = image::RgbImage::new(width, height);

    for (x, y, pixel) in image.enumerate_pixels() {
        let [r, g, b, a] = pixel.0;
        let alpha = a as f32 / 255.0;
        let blend = |channel: u8| -> u8 {
            ((channel as f32 * alpha) + (255.0 * (1.0 - alpha))).round() as u8
        };
        flattened.put_pixel(x, y, image::Rgb([blend(r), blend(g), blend(b)]));
    }

    flattened
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
    let candidates = [
        value.pointer("/choices/0/message/content"),
        value.pointer("/choices/0/text"),
        value.get("output_text"),
        value.pointer("/output/0/content"),
        value.pointer("/message/content"),
    ];

    for candidate in candidates.into_iter().flatten() {
        if let Some(text) = extract_text_fragments(candidate) {
            return Ok(text);
        }
    }

    Err(AppError::ResponseFormat(format!(
        "missing textual response content in payload: {}",
        preview_json(value)
    )))
}

fn extract_text_fragments(value: &Value) -> Option<String> {
    let mut fragments = Vec::new();
    collect_text_fragments(value, &mut fragments);
    let merged = fragments.join("\n").trim().to_owned();
    if merged.is_empty() {
        None
    } else {
        Some(merged)
    }
}

fn collect_text_fragments(value: &Value, fragments: &mut Vec<String>) {
    match value {
        Value::String(text) => {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                fragments.push(trimmed.to_owned());
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_text_fragments(item, fragments);
            }
        }
        Value::Object(map) => {
            for key in ["text", "content", "value", "output_text"] {
                if let Some(inner) = map.get(key) {
                    collect_text_fragments(inner, fragments);
                }
            }
        }
        _ => {}
    }
}

fn extract_models_from_response(value: &Value) -> Result<Vec<ModelDescriptor>, AppError> {
    let mut best = Vec::new();
    collect_best_model_list(value, &mut best);

    if best.is_empty() {
        return Err(AppError::ResponseFormat(format!(
            "model list payload did not contain a recognizable model array: {}",
            preview_json(value)
        )));
    }

    best.sort_by_key(|item| item.id.to_ascii_lowercase());
    best.dedup_by(|a, b| a.id == b.id);
    Ok(best)
}

fn collect_best_model_list(value: &Value, best: &mut Vec<ModelDescriptor>) {
    let parsed = parse_model_array_candidate(value);
    if parsed.len() > best.len() {
        *best = parsed;
    }

    match value {
        Value::Object(map) => {
            for key in [
                "data", "models", "items", "list", "results", "result", "payload", "body",
            ] {
                if let Some(inner) = map.get(key) {
                    collect_best_model_list(inner, best);
                }
            }
            for (key, inner) in map {
                if !matches!(
                    key.as_str(),
                    "data"
                        | "models"
                        | "items"
                        | "list"
                        | "results"
                        | "result"
                        | "payload"
                        | "body"
                ) {
                    collect_best_model_list(inner, best);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_best_model_list(item, best);
            }
        }
        _ => {}
    }
}

fn parse_model_array_candidate(value: &Value) -> Vec<ModelDescriptor> {
    match value {
        Value::Array(items) => items.iter().filter_map(parse_model_descriptor).collect(),
        _ => Vec::new(),
    }
}

fn parse_model_descriptor(value: &Value) -> Option<ModelDescriptor> {
    match value {
        Value::String(text) => {
            let id = text.trim();
            if looks_like_model_id(id) {
                Some(ModelDescriptor {
                    id: id.to_owned(),
                    owned_by: None,
                })
            } else {
                None
            }
        }
        Value::Object(map) => {
            let id = extract_stringish_field(map, &["id", "name", "model", "slug", "model_id"])?;
            if !looks_like_model_id(&id) {
                return None;
            }

            let owned_by = extract_stringish_field(
                map,
                &["owned_by", "provider", "vendor", "organization", "owner"],
            )
            .filter(|value| !value.is_empty());

            Some(ModelDescriptor { id, owned_by })
        }
        _ => None,
    }
}

fn extract_stringish_field(
    object: &serde_json::Map<String, Value>,
    keys: &[&str],
) -> Option<String> {
    for key in keys {
        if let Some(value) = object.get(*key) {
            if let Some(text) = extract_stringish_value(value) {
                let trimmed = text.trim().to_owned();
                if !trimmed.is_empty() {
                    return Some(trimmed);
                }
            }
        }
    }
    None
}

fn extract_stringish_value(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.to_owned()),
        Value::Number(number) => Some(number.to_string()),
        Value::Object(map) => {
            for key in ["id", "name", "value", "text"] {
                if let Some(inner) = map.get(key) {
                    if let Some(text) = extract_stringish_value(inner) {
                        return Some(text);
                    }
                }
            }
            None
        }
        _ => None,
    }
}

fn looks_like_model_id(text: &str) -> bool {
    let trimmed = text.trim();
    !trimmed.is_empty()
        && trimmed.len() <= 160
        && !trimmed.starts_with('<')
        && !trimmed.starts_with("http://")
        && !trimmed.starts_with("https://")
        && !trimmed.chars().any(char::is_whitespace)
        && trimmed
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':' | '/' | '+'))
}

#[derive(Clone, Serialize)]
struct ChatCompletionRequest {
    model: String,
    temperature: f32,
    max_tokens: u32,
    stream: bool,
    messages: Vec<ChatMessage>,
}

impl ChatCompletionRequest {
    fn from_request(request: &AnalysisRequest) -> Self {
        Self {
            model: request.model.clone(),
            temperature: 0.2,
            max_tokens: 160,
            stream: false,
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

#[derive(Clone, Serialize)]
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
                        "url": image_data_url,
                        "detail": "auto"
                    }
                }
            ]),
        }
    }
}
