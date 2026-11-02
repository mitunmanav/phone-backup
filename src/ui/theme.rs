use crate::*;

/// Windows 11 colours: the window, a lighter page beside the page list, lighter panels on it.
pub(crate) struct Tokens {
    /// The window: the page list and the command bar sit on it.
    pub(crate) bg: Color32,
    /// The page beside the page list: one step lighter than `bg`.
    pub(crate) layer: Color32,
    /// Panels on the page: one step lighter again.
    pub(crate) surface: Color32,
    pub(crate) field: Color32,
    pub(crate) line: Color32,
    /// Edges of fields, tick boxes and buttons: at least 3:1 against `surface` and `field`.
    pub(crate) border: Color32,
    /// Hovered and current rows of the page list.
    pub(crate) hover: Color32,
    pub(crate) ink: Color32,
    pub(crate) muted: Color32,
    /// Fill of the main button and progress, and links: at least 4.5:1 on every surface.
    pub(crate) accent: Color32,
    pub(crate) on_accent: Color32,
    /// Behind what a finished job says.
    pub(crate) info: Color32,
    pub(crate) error: Color32,
}

pub(crate) fn tokens(dark: bool) -> Tokens {
    let hex = |h: u32| Color32::from_rgb((h >> 16) as u8, (h >> 8) as u8, h as u8);
    if dark {
        Tokens {
            bg: hex(0x202020),
            layer: hex(0x272727),
            surface: hex(0x2E2E2E),
            field: hex(0x252525),
            line: hex(0x3C3C3C),
            border: hex(0x8A8A8A),
            hover: hex(0x363636),
            ink: hex(0xFFFFFF),
            muted: hex(0xB0B0B0),
            accent: hex(0x60CDFF),
            on_accent: hex(0x00131C),
            info: hex(0x1F3140),
            error: hex(0xFF99A4),
        }
    } else {
        Tokens {
            bg: hex(0xF3F3F3),
            layer: hex(0xFBFBFB),
            surface: hex(0xFFFFFF),
            field: hex(0xF9F9F9),
            line: hex(0xE0E0E0),
            border: hex(0x8A8A8A),
            hover: hex(0xEDEDED),
            ink: hex(0x1A1A1A),
            muted: hex(0x5E5E5E),
            accent: hex(0x0067C0),
            on_accent: hex(0xFFFFFF),
            info: hex(0xEAF2FB),
            error: hex(0xC42B1C),
        }
    }
}

pub(crate) fn visuals(dark: bool) -> egui::Visuals {
    let t = tokens(dark);
    let mut v = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    v.panel_fill = t.bg;
    v.window_fill = t.layer;
    v.extreme_bg_color = t.field;
    v.override_text_color = Some(t.ink);
    v.hyperlink_color = t.accent;
    v.selection.bg_fill = t.accent.gamma_multiply(0.35);
    v.selection.stroke = Stroke::new(2.0, t.accent);
    v.widgets.inactive.weak_bg_fill = t.surface;
    v.widgets.inactive.bg_fill = t.field;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, t.border);
    v.widgets.hovered.weak_bg_fill = t.hover;
    v.widgets.hovered.bg_fill = t.hover;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, t.accent);
    // egui draws keyboard focus with the pressed style; 2 px of ink makes it visible on every fill.
    v.widgets.active.bg_stroke = Stroke::new(2.0, t.ink);
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        // Windows 11: 4 px on controls, 8 px on dialogs and menus.
        w.corner_radius = 4.into();
    }
    v.window_corner_radius = 8.into();
    v.menu_corner_radius = 8.into();
    v
}

/// Windows' "Animation effects" setting. Off: pages and the tab pill change at once.
pub(crate) fn motion_on() -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SPI_GETCLIENTAREAANIMATION, SystemParametersInfoW,
        };
        let mut on = 1i32;
        // SAFETY: this action writes one BOOL through the pointer, which outlives the call.
        unsafe {
            SystemParametersInfoW(
                SPI_GETCLIENTAREAANIMATION,
                0,
                (&mut on as *mut i32).cast(),
                0,
            )
        };
        on != 0
    }
    #[cfg(not(windows))]
    true
}

pub(crate) fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("semibold".into()))
}
