use serde::{Deserialize, Serialize};

use crate::{
    domain::{AppError, ImageSourceKind, ParseStatus, ValidationIssue},
    state::{ModelCatalogState, RequestPhase, SaveState},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    #[default]
    ZhCn,
    EnUs,
    RuRu,
}

impl Language {
    pub const ALL: [Self; 3] = [Self::ZhCn, Self::EnUs, Self::RuRu];

    pub fn all() -> &'static [Self; 3] {
        &Self::ALL
    }

    pub fn native_label(self) -> &'static str {
        match self {
            Self::ZhCn => "简体中文",
            Self::EnUs => "English",
            Self::RuRu => "Русский",
        }
    }

    pub fn short_label(self) -> &'static str {
        match self {
            Self::ZhCn => "ZH",
            Self::EnUs => "EN",
            Self::RuRu => "RU",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextKey {
    AppTitle,
    AppSubtitle,
    SectionInterface,
    SectionInterfaceSub,
    SectionPrompt,
    SectionPromptSub,
    SectionIntake,
    SectionIntakeSub,
    LanguageLabel,
    LanguageHint,
    ShowSecret,
    HideSecret,
    BaseUrlLabel,
    ApiKeyLabel,
    ModelCatalogLabel,
    ModelLabel,
    ManualModelLabel,
    ManualModelHint,
    RequestTimeoutLabel,
    SecondsLabel,
    ApiKeyStorageHint,
    AutosaveHint,
    DefaultPromptBadge,
    ResetDefaultPrompt,
    AllowConfidenceNote,
    PromptRiskHint,
    ClipboardBullet,
    DragDropBullet,
    MemoryOnlyBullet,
    WindowWidthBullet,
    MainTaskArea,
    PasteAndAnalyze,
    MainCanvasSubtitle,
    MainCanvasAssist,
    DropToImport,
    ProcessStatus,
    ConfigIncomplete,
    Analyzing,
    StartLocate,
    SelectImage,
    ReplaceImage,
    ClearImage,
    ShortcutHint,
    ResultsPlaceholderTitle,
    ResultsPlaceholderSub,
    StructuredResult,
    CopyActions,
    CopyStructured,
    CopyFull,
    CopyRaw,
    ConfidencePanel,
    ConfidencePanelSub,
    RawOutput,
    RawOutputSub,
    ExpandRawOutput,
    CollapseRawOutput,
    ReviewHint,
    NotExtracted,
    ResultCountry,
    ResultDomestic,
    ResultCity,
    ResultPlace,
    FooterStatus,
    FooterImage,
    FooterLatency,
    FooterParse,
    FooterConfigPath,
    FooterGithubTooltip,
    ToastGithubOpened,
    ToastGithubOpenFailedPrefix,
    ModelRefresh,
    ModelRefreshLoading,
    ModelCatalogIdleHint,
    ModelCatalogLoadingHint,
    ModelCatalogEmptyHint,
    ManualModelToggle,
    ToastSupportImport,
    ToastClipboardLoaded,
    ToastImageNeedConfig,
    ToastRequestSubmitting,
    ToastRestoredPrompt,
    ToastClearedImage,
    ToastCopiedStructured,
    ToastCopiedFull,
    ToastCopiedRaw,
    ToastCopyFailedPrefix,
    ToastImageLoadedPrefix,
    ToastLanguageChangedPrefix,
    ToastModelCatalogLoadedPrefix,
    ToastModelSelectedPrefix,
    ToastModelCatalogStale,
    SaveIdle,
    SaveSaving,
    SaveSaved,
    SaveInvalidPrefix,
    SaveErrorPrefix,
    PhaseIdle,
    PhaseImageReady,
    PhasePreparing,
    PhaseRequesting,
    PhaseParseSuccess,
    PhaseParsePartial,
    PhaseFailed,
    ParseStrict,
    ParsePartial,
    ParseFallback,
    ParseStrictHint,
    ParsePartialHint,
    ParseFallbackHint,
    SourceClipboard,
    SourceDragDrop,
    SourceFilePicker,
    ValidationBaseUrlRequired,
    ValidationApiKeyRequired,
    ValidationModelRequired,
    ErrorTitleConfig,
    ErrorTitleClipboard,
    ErrorTitleImage,
    ErrorTitleNetwork,
    ErrorTitleAuth,
    ErrorTitleRate,
    ErrorTitleService,
    ErrorTitleResponse,
    ErrorConfigDirUnavailable,
    ErrorConfigStorePrefix,
    ErrorClipboardMissing,
    ErrorClipboardUnavailablePrefix,
    ErrorUnsupportedImagePrefix,
    ErrorImageProcessingPrefix,
    ErrorNetworkPrefix,
    ErrorAuthentication,
    ErrorRateLimited,
    ErrorServicePrefix,
    ErrorResponseFormatPrefix,
    ConfidencePrefix,
}

pub fn t(language: Language, key: TextKey) -> &'static str {
    match language {
        Language::ZhCn => zh(key),
        Language::EnUs => en(key),
        Language::RuRu => ru(key),
    }
}

pub fn default_system_prompt(language: Language) -> &'static str {
    match language {
        Language::ZhCn => {
            "你是地理识图定位助手。请根据图片内容推测地理位置，只输出一行结果，格式必须为 大洲某方位某国-国内区位大区-具体城市区位-具体地点。不要编号，不要 Markdown，不要解释，不要输出多余前缀。若无法完全确定，也必须给出最可信的层级描述，避免留空。仅当置信度明显不足时，第二行才允许输出 置信说明: 加不超过 18 个字的极短说明。"
        }
        Language::EnUs => {
            "You are a geo-visual locating assistant. Infer the location from the image and output exactly one line in the format ContinentDirectionCountry-DomesticRegion-CityRegion-SpecificPlace. Do not number items, do not use Markdown, do not explain, and do not prepend any label. Even if certainty is limited, still provide the most credible hierarchy instead of leaving segments empty. Only when confidence is clearly low may you add a second line in the form Confidence note: plus a very short note within 18 words."
        }
        Language::RuRu => {
            "Ты помощник по геовизуальной локализации. Определи местоположение по изображению и выведи ровно одну строку в формате КонтинентНаправлениеСтрана-ВнутренняяЗона-ГородскаяЗона-КонкретноеМесто. Не используй нумерацию, Markdown, пояснения и лишние префиксы. Даже при неполной уверенности выдай наиболее вероятную иерархию и не оставляй сегменты пустыми. Только если уверенность явно низкая, разрешена вторая строка в виде Примечание уверенности: плюс очень короткое пояснение не длиннее 18 слов."
        }
    }
}

pub fn default_user_prompt(language: Language) -> &'static str {
    match language {
        Language::ZhCn => {
            "请分析图片，输出最可信的位置层级。若无法精确到具体地点，也要尽量给出最稳妥的层级定位。"
        }
        Language::EnUs => {
            "Analyze the image and return the most credible location hierarchy. If the exact place is unclear, still provide the safest hierarchical estimate possible."
        }
        Language::RuRu => {
            "Проанализируй изображение и верни наиболее вероятную иерархию местоположения. Если точное место определить трудно, все равно дай максимально надежную иерархическую оценку."
        }
    }
}

pub fn confidence_prefix(language: Language) -> &'static str {
    t(language, TextKey::ConfidencePrefix)
}

pub fn validation_message(language: Language, issue: &ValidationIssue) -> &'static str {
    match issue {
        ValidationIssue::BaseUrlRequired => t(language, TextKey::ValidationBaseUrlRequired),
        ValidationIssue::ApiKeyRequired => t(language, TextKey::ValidationApiKeyRequired),
        ValidationIssue::ModelRequired => t(language, TextKey::ValidationModelRequired),
    }
}

pub fn image_source_label(language: Language, source: &ImageSourceKind) -> &'static str {
    match source {
        ImageSourceKind::Clipboard => t(language, TextKey::SourceClipboard),
        ImageSourceKind::DragDrop => t(language, TextKey::SourceDragDrop),
        ImageSourceKind::FilePicker => t(language, TextKey::SourceFilePicker),
    }
}

pub fn image_source_name(language: Language, source: &ImageSourceKind) -> String {
    match language {
        Language::ZhCn => format!("{}图片", image_source_label(language, source)),
        Language::EnUs => match source {
            ImageSourceKind::Clipboard => "Clipboard image".to_owned(),
            ImageSourceKind::DragDrop => "Dragged image".to_owned(),
            ImageSourceKind::FilePicker => "Imported image".to_owned(),
        },
        Language::RuRu => match source {
            ImageSourceKind::Clipboard => "Изображение из буфера".to_owned(),
            ImageSourceKind::DragDrop => "Перетащенное изображение".to_owned(),
            ImageSourceKind::FilePicker => "Импортированное изображение".to_owned(),
        },
    }
}

pub fn request_phase_label(language: Language, phase: &RequestPhase) -> &'static str {
    match phase {
        RequestPhase::Idle => t(language, TextKey::PhaseIdle),
        RequestPhase::ImageReady => t(language, TextKey::PhaseImageReady),
        RequestPhase::Preparing => t(language, TextKey::PhasePreparing),
        RequestPhase::Requesting => t(language, TextKey::PhaseRequesting),
        RequestPhase::ParseSuccess => t(language, TextKey::PhaseParseSuccess),
        RequestPhase::ParsePartial => t(language, TextKey::PhaseParsePartial),
        RequestPhase::Failed => t(language, TextKey::PhaseFailed),
    }
}

pub fn save_state_label(language: Language, state: &SaveState) -> String {
    match state {
        SaveState::Idle => t(language, TextKey::SaveIdle).to_owned(),
        SaveState::Saving => t(language, TextKey::SaveSaving).to_owned(),
        SaveState::Saved => t(language, TextKey::SaveSaved).to_owned(),
        SaveState::Invalid(issue) => {
            format!(
                "{} · {}",
                t(language, TextKey::SaveInvalidPrefix),
                validation_message(language, issue)
            )
        }
        SaveState::Error(error) => {
            format!(
                "{} · {}",
                t(language, TextKey::SaveErrorPrefix),
                error_message(language, error)
            )
        }
    }
}

pub fn parse_status_label(language: Language, status: &ParseStatus) -> &'static str {
    match status {
        ParseStatus::Strict => t(language, TextKey::ParseStrict),
        ParseStatus::Partial => t(language, TextKey::ParsePartial),
        ParseStatus::Fallback => t(language, TextKey::ParseFallback),
    }
}

pub fn parse_status_hint(language: Language, status: &ParseStatus) -> &'static str {
    match status {
        ParseStatus::Strict => t(language, TextKey::ParseStrictHint),
        ParseStatus::Partial => t(language, TextKey::ParsePartialHint),
        ParseStatus::Fallback => t(language, TextKey::ParseFallbackHint),
    }
}

pub fn error_title(language: Language, error: &AppError) -> &'static str {
    match error {
        AppError::ConfigDirectoryUnavailable
        | AppError::ConfigStore(_)
        | AppError::Validation(_) => t(language, TextKey::ErrorTitleConfig),
        AppError::ClipboardImageMissing | AppError::ClipboardUnavailable(_) => {
            t(language, TextKey::ErrorTitleClipboard)
        }
        AppError::UnsupportedImage(_) | AppError::ImageProcessing(_) => {
            t(language, TextKey::ErrorTitleImage)
        }
        AppError::Network(_) => t(language, TextKey::ErrorTitleNetwork),
        AppError::Authentication => t(language, TextKey::ErrorTitleAuth),
        AppError::RateLimited => t(language, TextKey::ErrorTitleRate),
        AppError::Service(_) => t(language, TextKey::ErrorTitleService),
        AppError::ResponseFormat(_) => t(language, TextKey::ErrorTitleResponse),
    }
}

pub fn error_message(language: Language, error: &AppError) -> String {
    match error {
        AppError::ConfigDirectoryUnavailable => {
            t(language, TextKey::ErrorConfigDirUnavailable).to_owned()
        }
        AppError::ConfigStore(message) => {
            format!(
                "{}{}",
                t(language, TextKey::ErrorConfigStorePrefix),
                message
            )
        }
        AppError::ClipboardImageMissing => t(language, TextKey::ErrorClipboardMissing).to_owned(),
        AppError::ClipboardUnavailable(message) => format!(
            "{}{}",
            t(language, TextKey::ErrorClipboardUnavailablePrefix),
            message
        ),
        AppError::UnsupportedImage(message) => {
            format!(
                "{}{}",
                t(language, TextKey::ErrorUnsupportedImagePrefix),
                message
            )
        }
        AppError::ImageProcessing(message) => {
            format!(
                "{}{}",
                t(language, TextKey::ErrorImageProcessingPrefix),
                message
            )
        }
        AppError::Validation(message) => message.clone(),
        AppError::Network(message) => {
            format!("{}{}", t(language, TextKey::ErrorNetworkPrefix), message)
        }
        AppError::Authentication => t(language, TextKey::ErrorAuthentication).to_owned(),
        AppError::RateLimited => t(language, TextKey::ErrorRateLimited).to_owned(),
        AppError::Service(message) => {
            format!("{}{}", t(language, TextKey::ErrorServicePrefix), message)
        }
        AppError::ResponseFormat(message) => {
            format!(
                "{}{}",
                t(language, TextKey::ErrorResponseFormatPrefix),
                message
            )
        }
    }
}

pub fn model_catalog_hint(language: Language, state: &ModelCatalogState, count: usize) -> String {
    match state {
        ModelCatalogState::Idle => t(language, TextKey::ModelCatalogIdleHint).to_owned(),
        ModelCatalogState::Loading => t(language, TextKey::ModelCatalogLoadingHint).to_owned(),
        ModelCatalogState::Ready => match language {
            Language::ZhCn => format!("已载入 {count} 个模型，可直接从目录选择。"),
            Language::EnUs => format!("Loaded {count} models. Pick one from the catalog."),
            Language::RuRu => format!("Загружено {count} моделей. Выберите нужную из каталога."),
        },
        ModelCatalogState::Empty => t(language, TextKey::ModelCatalogEmptyHint).to_owned(),
        ModelCatalogState::Error(error) => error_message(language, error),
    }
}

pub fn toast_model_catalog_loaded(language: Language, count: usize) -> String {
    match language {
        Language::ZhCn => format!(
            "{}{} 个模型",
            t(language, TextKey::ToastModelCatalogLoadedPrefix),
            count
        ),
        Language::EnUs => format!(
            "{}{} models",
            t(language, TextKey::ToastModelCatalogLoadedPrefix),
            count
        ),
        Language::RuRu => format!(
            "{}{} моделей",
            t(language, TextKey::ToastModelCatalogLoadedPrefix),
            count
        ),
    }
}

fn zh(key: TextKey) -> &'static str {
    match key {
        TextKey::AppTitle => "RGMR",
        TextKey::AppSubtitle => "多模态地理定位",
        TextKey::SectionInterface => "接口配置",
        TextKey::SectionInterfaceSub => "Base URL、API Key、模型与语言",
        TextKey::SectionPrompt => "提示词",
        TextKey::SectionPromptSub => "编辑或恢复默认提示词",
        TextKey::SectionIntake => "图片输入",
        TextKey::SectionIntakeSub => "粘贴、拖拽或选择文件",
        TextKey::LanguageLabel => "语言",
        TextKey::LanguageHint => "切换后会立即保存；若仍使用默认提示词，会同步切换默认语言版本。",
        TextKey::ShowSecret => "显示",
        TextKey::HideSecret => "隐藏",
        TextKey::BaseUrlLabel => "Base URL",
        TextKey::ApiKeyLabel => "API Key",
        TextKey::ModelCatalogLabel => "模型目录",
        TextKey::ModelLabel => "已选模型",
        TextKey::ManualModelLabel => "手动模型兜底",
        TextKey::ManualModelHint => "当服务端不暴露 /models 或目录异常时，可直接填写模型名。",
        TextKey::RequestTimeoutLabel => "请求超时",
        TextKey::SecondsLabel => "秒",
        TextKey::ApiKeyStorageHint => {
            "当前版本仍按明文保存 API Key 至本地 TOML，可后续升级为凭据保管。"
        }
        TextKey::AutosaveHint => "配置采用去抖自动保存，关闭窗口前会再次尝试落盘。",
        TextKey::DefaultPromptBadge => "默认提示词",
        TextKey::ResetDefaultPrompt => "重置默认提示词",
        TextKey::AllowConfidenceNote => "允许第二行置信说明",
        TextKey::PromptRiskHint => "当前提示词可能削弱结构化解析稳定性。",
        TextKey::ClipboardBullet => "支持 Ctrl+V 读取剪贴板图片",
        TextKey::DragDropBullet => "支持拖拽图片与文件选择导入",
        TextKey::MemoryOnlyBullet => "图片仅在内存中处理，不额外写回磁盘",
        TextKey::WindowWidthBullet => "窗口变窄时会自动切换布局",
        TextKey::MainTaskArea => "图片输入",
        TextKey::PasteAndAnalyze => "粘贴并识图",
        TextKey::MainCanvasSubtitle => "导入图片并输出结构化定位结果",
        TextKey::MainCanvasAssist => "支持 Ctrl+V 粘贴剪贴板图片",
        TextKey::DropToImport => "松开鼠标即可载入图片",
        TextKey::ProcessStatus => "处理状态",
        TextKey::ConfigIncomplete => "接口配置未完成，主 CTA 已暂时禁用。",
        TextKey::Analyzing => "正在分析...",
        TextKey::StartLocate => "开始定位",
        TextKey::SelectImage => "选择图片",
        TextKey::ReplaceImage => "重新粘贴",
        TextKey::ClearImage => "清空",
        TextKey::ShortcutHint => "快捷键：Ctrl+V 读取图片，Ctrl+Enter 发起定位。",
        TextKey::ResultsPlaceholderTitle => "定位结果",
        TextKey::ResultsPlaceholderSub => "识图完成后，这里会显示结构化结果与原始输出。",
        TextKey::StructuredResult => "定位结果",
        TextKey::CopyActions => "复制",
        TextKey::CopyStructured => "复制层级结果",
        TextKey::CopyFull => "复制完整结果",
        TextKey::CopyRaw => "复制原始输出",
        TextKey::ConfidencePanel => "解析说明",
        TextKey::ConfidencePanelSub => "解析状态与复核建议",
        TextKey::RawOutput => "原始输出",
        TextKey::RawOutputSub => "保留模型原文，便于校验与复制",
        TextKey::ExpandRawOutput => "展开原始输出",
        TextKey::CollapseRawOutput => "折叠原始输出",
        TextKey::ReviewHint => "当前结果建议结合原始输出人工复核。",
        TextKey::NotExtracted => "未稳定提取",
        TextKey::ResultCountry => "国家",
        TextKey::ResultDomestic => "国内区位",
        TextKey::ResultCity => "城市区位",
        TextKey::ResultPlace => "具体地点",
        TextKey::FooterStatus => "当前状态",
        TextKey::FooterImage => "图片",
        TextKey::FooterLatency => "耗时",
        TextKey::FooterParse => "解析",
        TextKey::FooterConfigPath => "配置落盘",
        TextKey::FooterGithubTooltip => "在默认浏览器中打开项目 GitHub 仓库",
        TextKey::ToastGithubOpened => "已在默认浏览器中打开 GitHub 仓库",
        TextKey::ToastGithubOpenFailedPrefix => "打开 GitHub 仓库失败：",
        TextKey::ModelRefresh => "刷新模型列表",
        TextKey::ModelRefreshLoading => "刷新中...",
        TextKey::ModelCatalogIdleHint => "填写 Base URL 与 API Key 后，点击刷新模型列表。",
        TextKey::ModelCatalogLoadingHint => "正在从服务器拉取可用模型...",
        TextKey::ModelCatalogEmptyHint => "接口返回了空模型列表，请检查账号权限或服务实现。",
        TextKey::ManualModelToggle => "显示手动模型兜底",
        TextKey::ToastSupportImport => "支持 Ctrl+V、拖拽导入与文件选择",
        TextKey::ToastClipboardLoaded => "已载入剪贴板图片",
        TextKey::ToastImageNeedConfig => "图片已就绪，请先补全接口配置后开始定位",
        TextKey::ToastRequestSubmitting => "正在提交视觉识图请求...",
        TextKey::ToastRestoredPrompt => "已恢复当前语言的默认提示词",
        TextKey::ToastClearedImage => "已清空当前图片",
        TextKey::ToastCopiedStructured => "已复制层级结果",
        TextKey::ToastCopiedFull => "已复制完整结果",
        TextKey::ToastCopiedRaw => "已复制原始输出",
        TextKey::ToastCopyFailedPrefix => "复制失败：",
        TextKey::ToastImageLoadedPrefix => "已载入图片 · ",
        TextKey::ToastLanguageChangedPrefix => "界面语言已切换为 ",
        TextKey::ToastModelCatalogLoadedPrefix => "模型目录已刷新，共 ",
        TextKey::ToastModelSelectedPrefix => "已从模型目录选择：",
        TextKey::ToastModelCatalogStale => "Base URL 或 API Key 已变更，模型目录已标记为待刷新。",
        TextKey::SaveIdle => "等待保存",
        TextKey::SaveSaving => "正在保存...",
        TextKey::SaveSaved => "已保存",
        TextKey::SaveInvalidPrefix => "未保存",
        TextKey::SaveErrorPrefix => "保存失败",
        TextKey::PhaseIdle => "等待图片",
        TextKey::PhaseImageReady => "可开始定位",
        TextKey::PhasePreparing => "正在准备图片",
        TextKey::PhaseRequesting => "正在分析地理线索",
        TextKey::PhaseParseSuccess => "解析成功",
        TextKey::PhaseParsePartial => "已容错解析",
        TextKey::PhaseFailed => "处理失败",
        TextKey::ParseStrict => "严格解析",
        TextKey::ParsePartial => "容错解析",
        TextKey::ParseFallback => "需要复核",
        TextKey::ParseStrictHint => "格式匹配稳定",
        TextKey::ParsePartialHint => "已做容错解析",
        TextKey::ParseFallbackHint => "建议人工复核",
        TextKey::SourceClipboard => "剪贴板",
        TextKey::SourceDragDrop => "拖拽导入",
        TextKey::SourceFilePicker => "文件选择",
        TextKey::ValidationBaseUrlRequired => "Base URL 不能为空",
        TextKey::ValidationApiKeyRequired => "API Key 不能为空",
        TextKey::ValidationModelRequired => "Model 不能为空",
        TextKey::ErrorTitleConfig => "配置问题",
        TextKey::ErrorTitleClipboard => "剪贴板问题",
        TextKey::ErrorTitleImage => "图片问题",
        TextKey::ErrorTitleNetwork => "网络问题",
        TextKey::ErrorTitleAuth => "鉴权失败",
        TextKey::ErrorTitleRate => "频率限制",
        TextKey::ErrorTitleService => "服务异常",
        TextKey::ErrorTitleResponse => "响应异常",
        TextKey::ErrorConfigDirUnavailable => "无法定位 %APPDATA% 目录，当前配置无法落盘。",
        TextKey::ErrorConfigStorePrefix => "配置读写失败：",
        TextKey::ErrorClipboardMissing => "未检测到剪贴板图片，请先复制截图或图片。",
        TextKey::ErrorClipboardUnavailablePrefix => "无法访问剪贴板：",
        TextKey::ErrorUnsupportedImagePrefix => "当前文件不是受支持的图片：",
        TextKey::ErrorImageProcessingPrefix => "图片处理失败：",
        TextKey::ErrorNetworkPrefix => "网络请求失败：",
        TextKey::ErrorAuthentication => "鉴权失败，请检查 Base URL、API Key 与模型权限。",
        TextKey::ErrorRateLimited => "请求过于频繁，请稍后再试。",
        TextKey::ErrorServicePrefix => "模型服务异常：",
        TextKey::ErrorResponseFormatPrefix => "模型返回格式异常：",
        TextKey::ConfidencePrefix => "置信说明",
    }
}

fn en(key: TextKey) -> &'static str {
    match key {
        TextKey::AppTitle => "RGMR",
        TextKey::AppSubtitle => "Multimodal geo-location",
        TextKey::SectionInterface => "API settings",
        TextKey::SectionInterfaceSub => "Base URL, API key, model, and language",
        TextKey::SectionPrompt => "Prompt",
        TextKey::SectionPromptSub => "Edit or restore the default prompt",
        TextKey::SectionIntake => "Image input",
        TextKey::SectionIntakeSub => "Paste, drag, or choose a file",
        TextKey::LanguageLabel => "Language",
        TextKey::LanguageHint => {
            "Changes are saved immediately. If the default prompt is still in use, its language version switches too."
        }
        TextKey::ShowSecret => "Show",
        TextKey::HideSecret => "Hide",
        TextKey::BaseUrlLabel => "Base URL",
        TextKey::ApiKeyLabel => "API Key",
        TextKey::ModelCatalogLabel => "Model catalog",
        TextKey::ModelLabel => "Selected model",
        TextKey::ManualModelLabel => "Manual model fallback",
        TextKey::ManualModelHint => {
            "If the server does not expose /models or the catalog fails, enter a model name manually."
        }
        TextKey::RequestTimeoutLabel => "Request timeout",
        TextKey::SecondsLabel => "sec",
        TextKey::ApiKeyStorageHint => {
            "This version still stores the API Key as plain text in local TOML. Credential vaulting can be added later."
        }
        TextKey::AutosaveHint => {
            "Configuration is saved with debounce and one final flush is attempted before exit."
        }
        TextKey::DefaultPromptBadge => "Default prompt",
        TextKey::ResetDefaultPrompt => "Reset default prompt",
        TextKey::AllowConfidenceNote => "Allow a second-line confidence note",
        TextKey::PromptRiskHint => "The current prompt may weaken structured parsing stability.",
        TextKey::ClipboardBullet => "Supports Ctrl+V image capture from the clipboard",
        TextKey::DragDropBullet => "Supports drag-and-drop images and file picker import",
        TextKey::MemoryOnlyBullet => "Images stay in memory only and are not written back to disk",
        TextKey::WindowWidthBullet => {
            "The layout adapts automatically when the window gets narrower"
        }
        TextKey::MainTaskArea => "Image input",
        TextKey::PasteAndAnalyze => "Paste and analyze",
        TextKey::MainCanvasSubtitle => "Import an image and return a structured location result",
        TextKey::MainCanvasAssist => "Supports Ctrl+V clipboard paste",
        TextKey::DropToImport => "Release the pointer to load the image",
        TextKey::ProcessStatus => "Processing status",
        TextKey::ConfigIncomplete => {
            "Interface configuration is incomplete, so the main CTA is temporarily disabled."
        }
        TextKey::Analyzing => "Analyzing...",
        TextKey::StartLocate => "Start locating",
        TextKey::SelectImage => "Select image",
        TextKey::ReplaceImage => "Paste again",
        TextKey::ClearImage => "Clear",
        TextKey::ShortcutHint => "Shortcuts: Ctrl+V loads an image, Ctrl+Enter starts locating.",
        TextKey::ResultsPlaceholderTitle => "Location result",
        TextKey::ResultsPlaceholderSub => {
            "Structured results and raw output will appear here after analysis."
        }
        TextKey::StructuredResult => "Location result",
        TextKey::CopyActions => "Copy",
        TextKey::CopyStructured => "Copy hierarchy",
        TextKey::CopyFull => "Copy full result",
        TextKey::CopyRaw => "Copy raw output",
        TextKey::ConfidencePanel => "Parse status",
        TextKey::ConfidencePanelSub => "Parsing quality and review hints",
        TextKey::RawOutput => "Raw output",
        TextKey::RawOutputSub => "Keep the original model text for review and copy",
        TextKey::ExpandRawOutput => "Expand raw output",
        TextKey::CollapseRawOutput => "Collapse raw output",
        TextKey::ReviewHint => "Review the raw output manually before trusting this result.",
        TextKey::NotExtracted => "Not extracted reliably",
        TextKey::ResultCountry => "Country",
        TextKey::ResultDomestic => "Domestic region",
        TextKey::ResultCity => "City region",
        TextKey::ResultPlace => "Specific place",
        TextKey::FooterStatus => "Status",
        TextKey::FooterImage => "Image",
        TextKey::FooterLatency => "Latency",
        TextKey::FooterParse => "Parsing",
        TextKey::FooterConfigPath => "Config path",
        TextKey::FooterGithubTooltip => "Open the project GitHub repository in the default browser",
        TextKey::ToastGithubOpened => "Opened the GitHub repository in the default browser",
        TextKey::ToastGithubOpenFailedPrefix => "Failed to open the GitHub repository: ",
        TextKey::ModelRefresh => "Refresh model list",
        TextKey::ModelRefreshLoading => "Refreshing...",
        TextKey::ModelCatalogIdleHint => {
            "Fill in Base URL and API Key, then refresh the model catalog."
        }
        TextKey::ModelCatalogLoadingHint => "Fetching available models from the server...",
        TextKey::ModelCatalogEmptyHint => {
            "The endpoint returned an empty model list. Check account permissions or server compatibility."
        }
        TextKey::ManualModelToggle => "Show manual model fallback",
        TextKey::ToastSupportImport => "Ctrl+V, drag-and-drop, and file picking are ready",
        TextKey::ToastClipboardLoaded => "Clipboard image loaded",
        TextKey::ToastImageNeedConfig => {
            "The image is ready. Complete the API configuration before locating."
        }
        TextKey::ToastRequestSubmitting => "Submitting the vision analysis request...",
        TextKey::ToastRestoredPrompt => "Restored the default prompt for the current language",
        TextKey::ToastClearedImage => "Current image cleared",
        TextKey::ToastCopiedStructured => "Structured hierarchy copied",
        TextKey::ToastCopiedFull => "Full result copied",
        TextKey::ToastCopiedRaw => "Raw output copied",
        TextKey::ToastCopyFailedPrefix => "Copy failed: ",
        TextKey::ToastImageLoadedPrefix => "Image loaded · ",
        TextKey::ToastLanguageChangedPrefix => "Interface language switched to ",
        TextKey::ToastModelCatalogLoadedPrefix => "Model catalog refreshed: ",
        TextKey::ToastModelSelectedPrefix => "Selected from catalog: ",
        TextKey::ToastModelCatalogStale => {
            "Base URL or API Key changed. The model catalog is now marked stale."
        }
        TextKey::SaveIdle => "Waiting to save",
        TextKey::SaveSaving => "Saving...",
        TextKey::SaveSaved => "Saved",
        TextKey::SaveInvalidPrefix => "Not saved",
        TextKey::SaveErrorPrefix => "Save failed",
        TextKey::PhaseIdle => "Waiting for image",
        TextKey::PhaseImageReady => "Ready to locate",
        TextKey::PhasePreparing => "Preparing image",
        TextKey::PhaseRequesting => "Analyzing geographic clues",
        TextKey::PhaseParseSuccess => "Parsed successfully",
        TextKey::PhaseParsePartial => "Parsed with fallback",
        TextKey::PhaseFailed => "Processing failed",
        TextKey::ParseStrict => "Strict parse",
        TextKey::ParsePartial => "Tolerant parse",
        TextKey::ParseFallback => "Needs review",
        TextKey::ParseStrictHint => "Format matched reliably",
        TextKey::ParsePartialHint => "Tolerance parsing applied",
        TextKey::ParseFallbackHint => "Manual review is recommended",
        TextKey::SourceClipboard => "Clipboard",
        TextKey::SourceDragDrop => "Drag-and-drop",
        TextKey::SourceFilePicker => "File picker",
        TextKey::ValidationBaseUrlRequired => "Base URL is required",
        TextKey::ValidationApiKeyRequired => "API Key is required",
        TextKey::ValidationModelRequired => "Model is required",
        TextKey::ErrorTitleConfig => "Configuration issue",
        TextKey::ErrorTitleClipboard => "Clipboard issue",
        TextKey::ErrorTitleImage => "Image issue",
        TextKey::ErrorTitleNetwork => "Network issue",
        TextKey::ErrorTitleAuth => "Authentication failed",
        TextKey::ErrorTitleRate => "Rate limited",
        TextKey::ErrorTitleService => "Service error",
        TextKey::ErrorTitleResponse => "Response error",
        TextKey::ErrorConfigDirUnavailable => {
            "Unable to locate the %APPDATA% directory, so configuration cannot be persisted."
        }
        TextKey::ErrorConfigStorePrefix => "Configuration read/write failed: ",
        TextKey::ErrorClipboardMissing => {
            "No image was found in the clipboard. Copy a screenshot or image first."
        }
        TextKey::ErrorClipboardUnavailablePrefix => "Unable to access the clipboard: ",
        TextKey::ErrorUnsupportedImagePrefix => "The selected file is not a supported image: ",
        TextKey::ErrorImageProcessingPrefix => "Image processing failed: ",
        TextKey::ErrorNetworkPrefix => "Network request failed: ",
        TextKey::ErrorAuthentication => {
            "Authentication failed. Check the Base URL, API Key, and model permissions."
        }
        TextKey::ErrorRateLimited => "Too many requests. Please try again later.",
        TextKey::ErrorServicePrefix => "Model service error: ",
        TextKey::ErrorResponseFormatPrefix => "Model response format error: ",
        TextKey::ConfidencePrefix => "Confidence note",
    }
}

fn ru(key: TextKey) -> &'static str {
    match key {
        TextKey::AppTitle => "RGMR",
        TextKey::AppSubtitle => "Мультимодальная геолокация",
        TextKey::SectionInterface => "Настройки API",
        TextKey::SectionInterfaceSub => "Base URL, API Key, модель и язык",
        TextKey::SectionPrompt => "Промпт",
        TextKey::SectionPromptSub => "Редактирование и сброс стандартного промпта",
        TextKey::SectionIntake => "Ввод изображения",
        TextKey::SectionIntakeSub => "Вставка, перетаскивание или выбор файла",
        TextKey::LanguageLabel => "Язык",
        TextKey::LanguageHint => {
            "Изменения сохраняются сразу. Если используется стандартный промпт, его языковая версия тоже переключится."
        }
        TextKey::ShowSecret => "Показать",
        TextKey::HideSecret => "Скрыть",
        TextKey::BaseUrlLabel => "Base URL",
        TextKey::ApiKeyLabel => "API Key",
        TextKey::ModelCatalogLabel => "Каталог моделей",
        TextKey::ModelLabel => "Выбранная модель",
        TextKey::ManualModelLabel => "Ручной резерв модели",
        TextKey::ManualModelHint => {
            "Если сервер не публикует /models или каталог недоступен, введите имя модели вручную."
        }
        TextKey::RequestTimeoutLabel => "Тайм-аут запроса",
        TextKey::SecondsLabel => "сек",
        TextKey::ApiKeyStorageHint => {
            "В этой версии API Key все еще хранится как открытый текст в локальном TOML. Позже можно перейти на безопасное хранилище."
        }
        TextKey::AutosaveHint => {
            "Конфигурация сохраняется с debounce, а перед выходом выполняется финальная попытка записи."
        }
        TextKey::DefaultPromptBadge => "Стандартный промпт",
        TextKey::ResetDefaultPrompt => "Сбросить стандартный промпт",
        TextKey::AllowConfidenceNote => "Разрешить вторую строку с пометкой уверенности",
        TextKey::PromptRiskHint => {
            "Текущий промпт может снизить стабильность структурного парсинга."
        }
        TextKey::ClipboardBullet => "Поддерживается Ctrl+V для чтения изображения из буфера обмена",
        TextKey::DragDropBullet => {
            "Поддерживаются перетаскивание изображения и импорт через выбор файла"
        }
        TextKey::MemoryOnlyBullet => {
            "Изображения обрабатываются только в памяти и не записываются обратно на диск"
        }
        TextKey::WindowWidthBullet => "При уменьшении окна макет перестраивается автоматически",
        TextKey::MainTaskArea => "Ввод изображения",
        TextKey::PasteAndAnalyze => "Вставить и анализировать",
        TextKey::MainCanvasSubtitle => {
            "Импортируйте изображение и получите структурированный результат локализации"
        }
        TextKey::MainCanvasAssist => "Поддерживается вставка изображения через Ctrl+V",
        TextKey::DropToImport => "Отпустите указатель, чтобы загрузить изображение",
        TextKey::ProcessStatus => "Статус обработки",
        TextKey::ConfigIncomplete => {
            "Конфигурация интерфейса неполная, поэтому основное действие временно отключено."
        }
        TextKey::Analyzing => "Идет анализ...",
        TextKey::StartLocate => "Начать локализацию",
        TextKey::SelectImage => "Выбрать изображение",
        TextKey::ReplaceImage => "Вставить заново",
        TextKey::ClearImage => "Очистить",
        TextKey::ShortcutHint => {
            "Горячие клавиши: Ctrl+V загружает изображение, Ctrl+Enter запускает локализацию."
        }
        TextKey::ResultsPlaceholderTitle => "Результат",
        TextKey::ResultsPlaceholderSub => {
            "После анализа здесь появятся структурированный результат и сырой вывод."
        }
        TextKey::StructuredResult => "Результат",
        TextKey::CopyActions => "Копирование",
        TextKey::CopyStructured => "Копировать иерархию",
        TextKey::CopyFull => "Копировать полный результат",
        TextKey::CopyRaw => "Копировать сырой вывод",
        TextKey::ConfidencePanel => "Статус разбора",
        TextKey::ConfidencePanelSub => "Качество разбора и подсказки для проверки",
        TextKey::RawOutput => "Сырой вывод",
        TextKey::RawOutputSub => "Исходный текст модели сохраняется для проверки и копирования",
        TextKey::ExpandRawOutput => "Развернуть сырой вывод",
        TextKey::CollapseRawOutput => "Свернуть сырой вывод",
        TextKey::ReviewHint => "Перед доверием к результату вручную проверьте сырой вывод.",
        TextKey::NotExtracted => "Надежно не извлечено",
        TextKey::ResultCountry => "Страна",
        TextKey::ResultDomestic => "Внутренняя зона",
        TextKey::ResultCity => "Городская зона",
        TextKey::ResultPlace => "Конкретное место",
        TextKey::FooterStatus => "Статус",
        TextKey::FooterImage => "Изображение",
        TextKey::FooterLatency => "Задержка",
        TextKey::FooterParse => "Парсинг",
        TextKey::FooterConfigPath => "Путь к конфигу",
        TextKey::FooterGithubTooltip => "Открыть GitHub-репозиторий проекта в браузере по умолчанию",
        TextKey::ToastGithubOpened => "GitHub-репозиторий открыт в браузере по умолчанию",
        TextKey::ToastGithubOpenFailedPrefix => "Не удалось открыть GitHub-репозиторий: ",
        TextKey::ModelRefresh => "Обновить список моделей",
        TextKey::ModelRefreshLoading => "Обновление...",
        TextKey::ModelCatalogIdleHint => {
            "Заполните Base URL и API Key, затем обновите каталог моделей."
        }
        TextKey::ModelCatalogLoadingHint => "Запрашиваются доступные модели с сервера...",
        TextKey::ModelCatalogEmptyHint => {
            "Эндпоинт вернул пустой список моделей. Проверьте права аккаунта или совместимость сервера."
        }
        TextKey::ManualModelToggle => "Показать ручной резерв модели",
        TextKey::ToastSupportImport => "Ctrl+V, drag-and-drop и выбор файла уже готовы",
        TextKey::ToastClipboardLoaded => "Изображение из буфера загружено",
        TextKey::ToastImageNeedConfig => {
            "Изображение готово. Завершите настройку API перед локализацией."
        }
        TextKey::ToastRequestSubmitting => "Отправляется запрос визуального анализа...",
        TextKey::ToastRestoredPrompt => "Восстановлен стандартный промпт для текущего языка",
        TextKey::ToastClearedImage => "Текущее изображение очищено",
        TextKey::ToastCopiedStructured => "Структурированная иерархия скопирована",
        TextKey::ToastCopiedFull => "Полный результат скопирован",
        TextKey::ToastCopiedRaw => "Сырой вывод скопирован",
        TextKey::ToastCopyFailedPrefix => "Ошибка копирования: ",
        TextKey::ToastImageLoadedPrefix => "Изображение загружено · ",
        TextKey::ToastLanguageChangedPrefix => "Язык интерфейса переключен на ",
        TextKey::ToastModelCatalogLoadedPrefix => "Каталог моделей обновлен: ",
        TextKey::ToastModelSelectedPrefix => "Выбрано из каталога: ",
        TextKey::ToastModelCatalogStale => {
            "Base URL или API Key изменились. Каталог моделей помечен как устаревший."
        }
        TextKey::SaveIdle => "Ожидание сохранения",
        TextKey::SaveSaving => "Сохранение...",
        TextKey::SaveSaved => "Сохранено",
        TextKey::SaveInvalidPrefix => "Не сохранено",
        TextKey::SaveErrorPrefix => "Ошибка сохранения",
        TextKey::PhaseIdle => "Ожидание изображения",
        TextKey::PhaseImageReady => "Готово к локализации",
        TextKey::PhasePreparing => "Подготовка изображения",
        TextKey::PhaseRequesting => "Анализ географических признаков",
        TextKey::PhaseParseSuccess => "Успешно разобрано",
        TextKey::PhaseParsePartial => "Разобрано с допуском",
        TextKey::PhaseFailed => "Сбой обработки",
        TextKey::ParseStrict => "Строгий разбор",
        TextKey::ParsePartial => "Толерантный разбор",
        TextKey::ParseFallback => "Нужна проверка",
        TextKey::ParseStrictHint => "Формат совпал надежно",
        TextKey::ParsePartialHint => "Применен допуск при разборе",
        TextKey::ParseFallbackHint => "Рекомендуется ручная проверка",
        TextKey::SourceClipboard => "Буфер обмена",
        TextKey::SourceDragDrop => "Перетаскивание",
        TextKey::SourceFilePicker => "Выбор файла",
        TextKey::ValidationBaseUrlRequired => "Base URL обязателен",
        TextKey::ValidationApiKeyRequired => "API Key обязателен",
        TextKey::ValidationModelRequired => "Model обязателен",
        TextKey::ErrorTitleConfig => "Проблема конфигурации",
        TextKey::ErrorTitleClipboard => "Проблема буфера обмена",
        TextKey::ErrorTitleImage => "Проблема изображения",
        TextKey::ErrorTitleNetwork => "Сетевая проблема",
        TextKey::ErrorTitleAuth => "Ошибка аутентификации",
        TextKey::ErrorTitleRate => "Лимит частоты",
        TextKey::ErrorTitleService => "Ошибка сервиса",
        TextKey::ErrorTitleResponse => "Ошибка ответа",
        TextKey::ErrorConfigDirUnavailable => {
            "Не удалось найти каталог %APPDATA%, поэтому конфигурацию нельзя сохранить."
        }
        TextKey::ErrorConfigStorePrefix => "Ошибка чтения или записи конфигурации: ",
        TextKey::ErrorClipboardMissing => {
            "Изображение в буфере обмена не найдено. Сначала скопируйте скриншот или картинку."
        }
        TextKey::ErrorClipboardUnavailablePrefix => "Нет доступа к буферу обмена: ",
        TextKey::ErrorUnsupportedImagePrefix => {
            "Выбранный файл не является поддерживаемым изображением: "
        }
        TextKey::ErrorImageProcessingPrefix => "Ошибка обработки изображения: ",
        TextKey::ErrorNetworkPrefix => "Сетевой запрос завершился ошибкой: ",
        TextKey::ErrorAuthentication => {
            "Аутентификация не прошла. Проверьте Base URL, API Key и права на модель."
        }
        TextKey::ErrorRateLimited => "Слишком много запросов. Повторите попытку позже.",
        TextKey::ErrorServicePrefix => "Ошибка сервиса модели: ",
        TextKey::ErrorResponseFormatPrefix => "Ошибка формата ответа модели: ",
        TextKey::ConfidencePrefix => "Примечание уверенности",
    }
}
