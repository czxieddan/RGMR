use std::time::Duration;

use eframe::{
    NativeOptions, Theme,
    egui::{IconData, ViewportBuilder},
};
use tracing_subscriber::{EnvFilter, fmt};

use crate::{
    services::ConfigStore,
    state::{AppState, SaveState},
    ui::RgmrApp,
};

pub fn run() {
    init_tracing();

    let mut viewport = ViewportBuilder::default()
        .with_title("RGMR")
        .with_inner_size([1600.0, 900.0])
        .with_min_inner_size([860.0, 680.0])
        .with_drag_and_drop(true)
        .with_decorations(false)
        .with_resizable(true)
        .with_transparent(true)
        .with_fullsize_content_view(true)
        .with_title_shown(false);

    if let Some(icon) = load_app_icon() {
        viewport = viewport.with_icon(icon);
    }

    let options = NativeOptions {
        viewport,
        centered: true,
        follow_system_theme: false,
        default_theme: Theme::Dark,
        ..Default::default()
    };

    let _ = eframe::run_native(
        "RGMR",
        options,
        Box::new(|cc| {
            let (store, state) = match ConfigStore::new() {
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

fn load_app_icon() -> Option<IconData> {
    let image = image::load_from_memory_with_format(
        include_bytes!("../resourses/app.ico"),
        image::ImageFormat::Ico,
    )
    .ok()?;
    let rgba = image.into_rgba8();
    let (width, height) = rgba.dimensions();

    Some(IconData {
        rgba: rgba.into_raw(),
        width,
        height,
    })
}
