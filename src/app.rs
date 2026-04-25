use std::time::Duration;

use eframe::{
    NativeOptions, Theme,
    egui::ViewportBuilder,
};
use tracing_subscriber::{EnvFilter, fmt};

use crate::{
    i18n::{self, TextKey},
    services::ConfigStore,
    state::{AppState, SaveState, ToastTone},
    ui::RgmrApp,
};

pub fn run() {
    init_tracing();

    let options = NativeOptions {
        viewport: ViewportBuilder::default()
            .with_title("RGMR")
            .with_inner_size([1460.0, 920.0])
            .with_min_inner_size([860.0, 680.0])
            .with_drag_and_drop(true)
            .with_decorations(false)
            .with_resizable(true)
            .with_transparent(true)
            .with_fullsize_content_view(true)
            .with_title_shown(false),
        centered: true,
        follow_system_theme: false,
        default_theme: Theme::Dark,
        ..Default::default()
    };

    let _ = eframe::run_native(
        "RGMR",
        options,
        Box::new(|cc| {
            let (store, mut state) = match ConfigStore::new() {
                Ok(store) => {
                    let state = match store.load() {
                        Ok(config) => AppState::new(config),
                        Err(err) => {
                            let mut fallback = AppState::new(Default::default());
                            fallback.save_state = SaveState::Error(err.clone());
                            fallback.set_error(err);
                            fallback
                        }
                    };
                    (Some(store), state)
                }
                Err(err) => {
                    let mut fallback = AppState::new(Default::default());
                    fallback.save_state = SaveState::Error(err.clone());
                    fallback.set_error(err);
                    (None, fallback)
                }
            };

            let language = state.config.ui.language;
            state.push_toast(ToastTone::Accent, i18n::t(language, TextKey::ToastSupportImport));

            Box::new(RgmrApp::new(
                cc.egui_ctx.clone(),
                state,
                store,
                Duration::from_millis(700),
            ))
        }),
    );
}

fn init_tracing() {
    let _ = fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .compact()
        .try_init();
}
