// Hide the cmd window on Windows release builds; keep it for debug so tracing
// shows up in the console.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use anyhow::Result;
use tracing_subscriber::EnvFilter;

mod app;
mod canvas;
mod commands;
mod direct_select;
mod interaction;
mod menu;
mod panels;
mod pen;
mod snap;
mod text;
mod tools;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                EnvFilter::new("info,wgpu_core=warn,wgpu_hal=warn,naga=warn,eframe=warn")
            }),
        )
        .with_target(false)
        .init();

    tracing::info!("Illuminator starting");

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Illuminator")
            .with_inner_size([1400.0, 900.0])
            .with_min_inner_size([900.0, 600.0]),
        vsync: true,
        ..Default::default()
    };

    eframe::run_native(
        "Illuminator",
        native_options,
        Box::new(|cc| Ok(Box::new(app::IlluminatorApp::new(cc)))),
    )
    .map_err(|e| anyhow::anyhow!("eframe error: {e}"))?;

    Ok(())
}
