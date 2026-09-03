use bevy::prelude::*;

#[derive(Clone, Resource)]
pub(crate) struct UiFont(pub(crate) Handle<Font>);

pub(crate) fn setup_ui_font(mut commands: Commands, mut fonts: ResMut<Assets<Font>>) {
    for path in font_candidates() {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        let Ok(font) = Font::try_from_bytes(bytes) else {
            continue;
        };
        commands.insert_resource(UiFont(fonts.add(font)));
        info!("UI font: {path}");
        return;
    }

    warn!("no system font with Cyrillic support found; falling back to Bevy default font");
    commands.insert_resource(UiFont(Handle::default()));
}

#[cfg(target_os = "linux")]
fn font_candidates() -> &'static [&'static str] {
    &[
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
        "/usr/share/fonts/truetype/liberation2/LiberationSans-Regular.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
    ]
}

#[cfg(target_os = "windows")]
fn font_candidates() -> &'static [&'static str] {
    &[
        r"C:\Windows\Fonts\segoeui.ttf",
        r"C:\Windows\Fonts\arial.ttf",
    ]
}

#[cfg(target_os = "macos")]
fn font_candidates() -> &'static [&'static str] {
    &[
        "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        "/System/Library/Fonts/Menlo.ttc",
    ]
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
fn font_candidates() -> &'static [&'static str] {
    &[]
}
