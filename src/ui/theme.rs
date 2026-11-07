use crate::*;

/// Neutral greys with copper as the only hue, apart from warnings and errors.
pub(crate) struct Tokens {
    /// The window: the page list and the command bar sit on it.
    pub(crate) bg: Color32,
    /// The page beside the page list: flush with `bg`.
    pub(crate) layer: Color32,
    /// Panels and dialogs on the page: one step away from `bg`.
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
    /// The light running along the backup cable.
    pub(crate) glint: Color32,
    /// Behind what a finished job says.
    pub(crate) info: Color32,
    /// Needs attention but nothing has failed, such as low battery.
    pub(crate) warning: Color32,
    pub(crate) error: Color32,
}

/// How far dark mode's greys sit above pure black, chosen in Settings.
pub(crate) static GREY: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
/// Lighter than this and red error text on a hovered row drops below 4.5:1.
pub(crate) const GREY_MAX: u8 = 28;

pub(crate) fn tokens(dark: bool) -> Tokens {
    let hex = |h: u32| Color32::from_rgb((h >> 16) as u8, (h >> 8) as u8, h as u8);
    if dark {
        // Red, green and blue stay equal, so a lighter dark mode never leans to a colour.
        let g = GREY.load(Ordering::Relaxed).min(GREY_MAX);
        let n = |v: u8| Color32::from_gray(v + g);
        Tokens {
            bg: n(0x00),
            layer: n(0x00),
            surface: n(0x10),
            field: n(0x00),
            line: n(0x2C),
            border: hex(0x8C8C8C),
            hover: n(0x1E),
            ink: hex(0xF4F4F4),
            muted: hex(0xB0B0B0),
            accent: hex(0xE39C66),
            on_accent: hex(0x1A0E05),
            glint: hex(0xFFE0C4),
            info: hex(0x29211A),
            warning: hex(0xE8B44C),
            error: hex(0xF2857B),
        }
    } else {
        Tokens {
            bg: hex(0xF5F5F5),
            layer: hex(0xF5F5F5),
            surface: hex(0xFFFFFF),
            field: hex(0xF5F5F5),
            line: hex(0xE2E2E2),
            border: hex(0x858585),
            hover: hex(0xECECEC),
            ink: hex(0x141414),
            muted: hex(0x5C5C5C),
            accent: hex(0xA5531E),
            on_accent: hex(0xFFFFFF),
            glint: hex(0xE39C66),
            info: hex(0xF8F1ED),
            warning: hex(0x8A5A00),
            error: hex(0xB3261E),
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
    v.window_fill = t.surface;
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

/// Windows' "Animation effects" setting. Off: pages and the page marker change at once.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn contrast(a: Color32, b: Color32) -> f32 {
        let lum = |c: Color32| {
            let c = egui::Rgba::from(c);
            0.2126 * c.r() + 0.7152 * c.g() + 0.0722 * c.b()
        };
        let (a, b) = (lum(a), lum(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    #[test]
    fn text_and_edges_stay_readable_in_both_themes() {
        for (dark, grey) in [(false, 0), (true, 0), (true, GREY_MAX)] {
            GREY.store(grey, Ordering::Relaxed);
            let t = tokens(dark);
            for back in [t.bg, t.surface, t.field, t.hover, t.info] {
                for text in [t.ink, t.muted, t.accent, t.warning, t.error] {
                    assert!(contrast(text, back) >= 4.5, "{text:?} on {back:?}");
                }
            }
            assert!(contrast(t.on_accent, t.accent) >= 4.5);
            for back in [t.bg, t.surface, t.field] {
                assert!(contrast(t.border, back) >= 3.0, "edge on {back:?}");
            }
        }
        GREY.store(0, Ordering::Relaxed);
    }
}
