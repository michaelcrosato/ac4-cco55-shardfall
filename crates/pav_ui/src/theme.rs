//! Colours, type sizes and fonts: [`Theme`] (data, `games/blacktap/ui/theme.toml`) and
//! [`install`], which registers the embedded fonts and an egui style with a context.
//!
//! Font families: `FontFamily::Proportional` is Alegreya Sans (body and HUD),
//! `FontFamily::Name("bold")` is Alegreya Sans Bold and `FontFamily::Name("display")` is
//! Pirata One (titles). egui's default fonts stay behind them as fallbacks for symbols.
//! Sizes are points at 1080p (the app sets `pixels_per_point = screen_height_px / 1080`).

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::{Context as _, Result, bail};
use egui::{Color32, FontFamily, FontId};
use pav_core::play::MarkerKind;
use serde::Deserialize;

use crate::settings::Colorblind;

/// The display face family name (Pirata One).
pub const DISPLAY: &str = "display";
/// The bold body family name (Alegreya Sans Bold).
pub const BOLD: &str = "bold";

static PIRATA_ONE: &[u8] = include_bytes!("../../../assets/fonts/PirataOne/PirataOne-Regular.ttf");
static ALEGREYA_SANS: &[u8] = include_bytes!("../../../assets/fonts/AlegreyaSans/AlegreyaSans-Regular.ttf");
static ALEGREYA_SANS_BOLD: &[u8] = include_bytes!("../../../assets/fonts/AlegreyaSans/AlegreyaSans-Bold.ttf");

/// Type sizes in points at 1080p.
#[derive(Clone, Debug, PartialEq)]
pub struct Sizes {
    /// The logo's main line.
    pub logo: f32,
    /// Screen titles (≥ 56).
    pub title: f32,
    /// Panel headings.
    pub heading: f32,
    /// Menu items.
    pub item: f32,
    /// Body text (≥ 28).
    pub body: f32,
    /// Labels and small print (≥ 24).
    pub label: f32,
    /// Hint bar text.
    pub hint: f32,
    /// Glyph height next to body text.
    pub glyph: f32,
    /// Banner text ("FORT CAPTURED").
    pub banner: f32,
}

impl Default for Sizes {
    fn default() -> Self {
        Self {
            logo: 150.0,
            title: 72.0,
            heading: 56.0,
            item: 56.0,
            body: 30.0,
            label: 26.0,
            hint: 28.0,
            glyph: 42.0,
            banner: 120.0,
        }
    }
}

/// The UI theme. Colours are egui sRGB colours; every derived shade has its own field so a
/// theme file can override it.
#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub parchment: Color32,
    pub parchment_dark: Color32,
    pub parchment_light: Color32,
    pub ink: Color32,
    pub ink_soft: Color32,
    /// Focus accent (rum amber).
    pub amber: Color32,
    pub amber_bright: Color32,
    pub teal: Color32,
    pub gold: Color32,
    /// Danger.
    pub red: Color32,
    pub rope: Color32,
    pub rope_dark: Color32,
    pub rope_light: Color32,
    pub green: Color32,
    /// Allies.
    pub blue: Color32,
    pub brass: Color32,
    pub wood: Color32,
    pub sizes: Sizes,
    /// Safe-area margin as a fraction of each screen side (TV overscan).
    pub safe_area: f32,
    /// Scale of the focused element.
    pub focus_scale: f32,
    /// Period of the "Press ✕" pulse (s).
    pub pulse_seconds: f32,
}

impl Default for Theme {
    fn default() -> Self {
        let c = |s: &str| hex(s).unwrap_or(Color32::MAGENTA);
        Self {
            parchment: c("#f1e3c6"),
            parchment_dark: c("#d8c196"),
            parchment_light: c("#fbf3e1"),
            ink: c("#2b1d12"),
            ink_soft: c("#5a4330"),
            amber: c("#c9822b"),
            amber_bright: c("#f0a845"),
            teal: c("#1f6f78"),
            gold: c("#e2b04a"),
            red: c("#a8322d"),
            rope: c("#a07a48"),
            rope_dark: c("#5c4024"),
            rope_light: c("#d0aa70"),
            green: c("#5f8f3e"),
            blue: c("#4f8fd6"),
            brass: c("#c89b3c"),
            wood: c("#6b4526"),
            sizes: Sizes::default(),
            safe_area: 0.05,
            focus_scale: 1.08,
            pulse_seconds: 1.6,
        }
    }
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct ThemeFile {
    #[serde(default)]
    colors: BTreeMap<String, String>,
    #[serde(default)]
    sizes: BTreeMap<String, f32>,
    #[serde(default)]
    layout: BTreeMap<String, f32>,
}

impl Theme {
    /// Reads a theme file: `[colors]` (name = "#rrggbb"), `[sizes]` (points) and `[layout]`
    /// (`safe_area`, `focus_scale`, `pulse_seconds`). Missing entries keep the defaults;
    /// unknown names and bad values are errors.
    pub fn from_toml(text: &str) -> Result<Theme> {
        let file: ThemeFile = toml::from_str(text).context("theme.toml")?;
        let mut t = Theme::default();
        for (name, value) in &file.colors {
            let slot = match name.as_str() {
                "parchment" => &mut t.parchment,
                "parchment_dark" => &mut t.parchment_dark,
                "parchment_light" => &mut t.parchment_light,
                "ink" => &mut t.ink,
                "ink_soft" => &mut t.ink_soft,
                "amber" => &mut t.amber,
                "amber_bright" => &mut t.amber_bright,
                "teal" => &mut t.teal,
                "gold" => &mut t.gold,
                "red" => &mut t.red,
                "rope" => &mut t.rope,
                "rope_dark" => &mut t.rope_dark,
                "rope_light" => &mut t.rope_light,
                "green" => &mut t.green,
                "blue" => &mut t.blue,
                "brass" => &mut t.brass,
                "wood" => &mut t.wood,
                _ => bail!("theme.toml: unknown colour `{name}`"),
            };
            *slot = hex(value).with_context(|| format!("theme.toml: colour `{name}` = `{value}` is not #rrggbb"))?;
        }
        for (name, &value) in &file.sizes {
            let s = &mut t.sizes;
            let slot = match name.as_str() {
                "logo" => &mut s.logo,
                "title" => &mut s.title,
                "heading" => &mut s.heading,
                "item" => &mut s.item,
                "body" => &mut s.body,
                "label" => &mut s.label,
                "hint" => &mut s.hint,
                "glyph" => &mut s.glyph,
                "banner" => &mut s.banner,
                _ => bail!("theme.toml: unknown size `{name}`"),
            };
            if !(4.0..=400.0).contains(&value) {
                bail!("theme.toml: size `{name}` = {value} is out of range");
            }
            *slot = value;
        }
        for (name, &value) in &file.layout {
            match name.as_str() {
                "safe_area" => t.safe_area = value.clamp(0.0, 0.2),
                "focus_scale" => t.focus_scale = value.clamp(1.0, 1.5),
                "pulse_seconds" => t.pulse_seconds = value.max(0.2),
                _ => bail!("theme.toml: unknown layout value `{name}`"),
            }
        }
        Ok(t)
    }

    /// Pirata One at `size` points.
    pub fn display(&self, size: f32) -> FontId {
        FontId::new(size, FontFamily::Name(DISPLAY.into()))
    }

    /// Alegreya Sans at `size` points.
    pub fn body(&self, size: f32) -> FontId {
        FontId::new(size, FontFamily::Proportional)
    }

    /// Alegreya Sans Bold at `size` points.
    pub fn bold(&self, size: f32) -> FontId {
        FontId::new(size, FontFamily::Name(BOLD.into()))
    }

    /// The colour of a world/compass marker, with the colour-blind palette and the
    /// high-contrast option applied.
    pub fn marker_color(&self, kind: MarkerKind, colorblind: Colorblind, high_contrast: bool) -> Color32 {
        let c = |s: &str| hex(s).unwrap_or(Color32::WHITE);
        let base = match colorblind {
            Colorblind::Off => match kind {
                MarkerKind::Objective | MarkerKind::Target => self.gold,
                MarkerKind::Waypoint => c("#5fc7c9"),
                MarkerKind::Enemy | MarkerKind::Threat => c("#d8463d"),
                MarkerKind::Ally => self.blue,
                MarkerKind::Collectible | MarkerKind::Interact => self.parchment_light,
            },
            // Okabe-Ito colours that stay apart for red-green colour blindness.
            Colorblind::Protanopia | Colorblind::Deuteranopia => match kind {
                MarkerKind::Objective | MarkerKind::Target => c("#f0e442"),
                MarkerKind::Waypoint => c("#56b4e9"),
                MarkerKind::Enemy | MarkerKind::Threat => c("#e69f00"),
                MarkerKind::Ally => c("#0072b2"),
                MarkerKind::Collectible | MarkerKind::Interact => c("#ffffff"),
            },
            // Blue-yellow colour blindness: reds and pinks against teal.
            Colorblind::Tritanopia => match kind {
                MarkerKind::Objective | MarkerKind::Target => c("#cc79a7"),
                MarkerKind::Waypoint => c("#009e73"),
                MarkerKind::Enemy | MarkerKind::Threat => c("#d55e00"),
                MarkerKind::Ally => c("#56b4e9"),
                MarkerKind::Collectible | MarkerKind::Interact => c("#ffffff"),
            },
        };
        if high_contrast { saturate(base) } else { base }
    }

    /// The danger colour under a colour-blind palette (health, threat pips, "!").
    pub fn danger(&self, colorblind: Colorblind) -> Color32 {
        self.marker_color(MarkerKind::Enemy, colorblind, false)
    }

    /// The warning colour ("?" awareness).
    pub fn warning(&self, colorblind: Colorblind) -> Color32 {
        match colorblind {
            Colorblind::Tritanopia => hex("#ff9db0").unwrap_or(self.gold),
            _ => hex("#f2cf4a").unwrap_or(self.gold),
        }
    }
}

/// Pushes a colour toward full saturation and brightness (the high-contrast HUD).
fn saturate(c: Color32) -> Color32 {
    let (r, g, b) = (c.r() as f32, c.g() as f32, c.b() as f32);
    let max = r.max(g).max(b).max(1.0);
    let k = 255.0 / max;
    Color32::from_rgb((r * k) as u8, (g * k) as u8, (b * k) as u8)
}

/// Parses `#rrggbb` or `#rrggbbaa` (the leading `#` is optional).
pub fn hex(s: &str) -> Option<Color32> {
    let s = s.trim().trim_start_matches('#');
    let byte = |i: usize| u8::from_str_radix(s.get(i..i + 2)?, 16).ok();
    match s.len() {
        6 => Some(Color32::from_rgb(byte(0)?, byte(2)?, byte(4)?)),
        8 => Some(Color32::from_rgba_unmultiplied(byte(0)?, byte(2)?, byte(4)?, byte(6)?)),
        _ => None,
    }
}

/// `color` with its alpha multiplied by `a` (0..1), for egui's premultiplied colours.
pub fn alpha(color: Color32, a: f32) -> Color32 {
    color.gamma_multiply(a.clamp(0.0, 1.0))
}

/// Linear blend between two colours in sRGB space (t 0..1).
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_premultiplied(f(a.r(), b.r()), f(a.g(), b.g()), f(a.b(), b.b()), f(a.a(), b.a()))
}

/// The font definitions [`install`] registers (exposed for hosts that build their own).
pub fn font_definitions() -> egui::FontDefinitions {
    let mut defs = egui::FontDefinitions::default();
    defs.font_data.insert("pirata_one".into(), Arc::new(egui::FontData::from_static(PIRATA_ONE)));
    defs.font_data.insert("alegreya_sans".into(), Arc::new(egui::FontData::from_static(ALEGREYA_SANS)));
    defs.font_data.insert("alegreya_sans_bold".into(), Arc::new(egui::FontData::from_static(ALEGREYA_SANS_BOLD)));
    let fallbacks: Vec<String> = defs.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let family = |first: &[&str]| first.iter().map(|s| s.to_string()).chain(fallbacks.iter().cloned()).collect::<Vec<_>>();
    defs.families.insert(FontFamily::Proportional, family(&["alegreya_sans"]));
    defs.families.insert(FontFamily::Name(BOLD.into()), family(&["alegreya_sans_bold"]));
    defs.families.insert(FontFamily::Name(DISPLAY.into()), family(&["pirata_one", "alegreya_sans_bold"]));
    if let Some(mono) = defs.families.get_mut(&FontFamily::Monospace) {
        mono.push("alegreya_sans".into());
    }
    defs
}

/// Registers the fonts and a matching egui style (text sizes, parchment-on-ink visuals) on
/// `ctx`. Call it once after creating the context, and again if the theme changes.
/// Also turns off egui's Ctrl +/- zoom: the app owns `pixels_per_point`.
pub fn install(ctx: &egui::Context, theme: &Theme) {
    ctx.set_fonts(font_definitions());
    ctx.options_mut(|o| o.zoom_with_keyboard = false);
    let t = theme.clone();
    ctx.all_styles_mut(move |style| {
        use egui::TextStyle;
        style.text_styles = [
            (TextStyle::Heading, t.display(t.sizes.heading)),
            (TextStyle::Body, t.body(t.sizes.body)),
            (TextStyle::Button, t.bold(t.sizes.body)),
            (TextStyle::Small, t.body(t.sizes.label)),
            (TextStyle::Monospace, FontId::monospace(t.sizes.label)),
        ]
        .into();
        let v = &mut style.visuals;
        *v = egui::Visuals::dark();
        v.override_text_color = Some(t.parchment);
        v.panel_fill = Color32::TRANSPARENT;
        v.window_fill = alpha(t.ink, 0.92);
        v.window_stroke = egui::Stroke::new(2.0, t.rope);
        v.selection.bg_fill = t.amber;
        v.selection.stroke = egui::Stroke::new(2.0, t.amber_bright);
        v.hyperlink_color = t.gold;
        v.widgets.hovered.bg_stroke = egui::Stroke::new(2.0, t.amber);
        v.widgets.active.bg_fill = t.amber;
        style.spacing.item_spacing = egui::vec2(12.0, 10.0);
        style.spacing.button_padding = egui::vec2(16.0, 10.0);
        style.interaction.tooltip_delay = 0.2;
    });
}
