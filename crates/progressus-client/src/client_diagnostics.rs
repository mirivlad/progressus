use std::time::{Duration, Instant};

use bevy::diagnostic::{
    Diagnostic, DiagnosticPath, Diagnostics, EntityCountDiagnosticsPlugin,
    FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin, RegisterDiagnostic,
};
use bevy::prelude::*;
use bevy::render::RenderApp;
use bevy::render::diagnostic::RenderDiagnosticsPlugin;
use bevy::render::renderer::RenderAdapterInfo;

use crate::render::PresentationCache;

pub(crate) const CLIENT_UPDATE_MS: DiagnosticPath =
    DiagnosticPath::const_new("progressus/client_update_ms");
pub(crate) const AUTHORITY_MS: DiagnosticPath =
    DiagnosticPath::const_new("progressus/authority_ms");
pub(crate) const PRESENTATION_MS: DiagnosticPath =
    DiagnosticPath::const_new("progressus/presentation_ms");
pub(crate) const TERRAIN_CHUNKS: DiagnosticPath =
    DiagnosticPath::const_new("progressus/terrain_chunks");

#[derive(Resource, Default)]
pub(crate) struct ClientUpdateTimer(Option<Instant>);

pub(crate) struct ProgressusDiagnosticsPlugin;

impl Plugin for ProgressusDiagnosticsPlugin {
    fn build(&self, app: &mut App) {
        app.register_diagnostic(Diagnostic::new(CLIENT_UPDATE_MS).with_suffix("ms"))
            .register_diagnostic(Diagnostic::new(AUTHORITY_MS).with_suffix("ms"))
            .register_diagnostic(Diagnostic::new(PRESENTATION_MS).with_suffix("ms"))
            .register_diagnostic(Diagnostic::new(TERRAIN_CHUNKS))
            .add_plugins((
                FrameTimeDiagnosticsPlugin::default(),
                EntityCountDiagnosticsPlugin::default(),
                RenderDiagnosticsPlugin,
                LogDiagnosticsPlugin {
                    wait_duration: Duration::from_secs(2),
                    ..Default::default()
                },
            ));
    }

    fn finish(&self, app: &mut App) {
        let Some(render_app) = app.get_sub_app(RenderApp) else {
            return;
        };
        if let Some(adapter) = render_app.world().get_resource::<RenderAdapterInfo>() {
            info!("Progressus diagnostics render adapter: {:?}", adapter.0);
        }
    }
}

pub(crate) fn begin_client_update(mut timer: ResMut<ClientUpdateTimer>) {
    timer.0 = Some(Instant::now());
}

pub(crate) fn end_client_update(
    mut timer: ResMut<ClientUpdateTimer>,
    cache: Res<PresentationCache>,
    mut diagnostics: Diagnostics,
) {
    if let Some(started) = timer.0.take() {
        diagnostics.add_measurement(&CLIENT_UPDATE_MS, || {
            started.elapsed().as_secs_f64() * 1000.0
        });
    }
    diagnostics.add_measurement(&TERRAIN_CHUNKS, || cache.terrain_chunks.len() as f64);
}

pub(crate) fn record_elapsed(
    diagnostics: &mut Diagnostics,
    path: &DiagnosticPath,
    started: Instant,
) {
    diagnostics.add_measurement(path, || started.elapsed().as_secs_f64() * 1000.0);
}
