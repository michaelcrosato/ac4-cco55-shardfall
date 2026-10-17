//! Drawing the front-end screens.

use std::f32::consts::TAU;

use egui::{Align2, Color32, Painter, Pos2, Rect, Stroke, Vec2, pos2, vec2};
use pav_core::play::{Objective, PlayFrame};

use super::{
    ATTRACT_FADE, CARD_SECONDS, ConfirmKind, FrontEnd, Hit, MOBILE_ITEMS, MenuItem, MobileItem, Mode, PAUSE_ITEMS, TITLE_FADE,
    Target,
};
use crate::bindings::{Bindings, PadButton};
use crate::glyph::{self, Glyph, GlyphFamily, Piece};
use crate::hud::{self, HudEnv, nav_hints};
use crate::input::Nav;
use crate::map;
use crate::paint::{self, LogoStyle, quantize};
use crate::settings::{Row, RowValue, TABS};
use crate::theme::{Theme, alpha, mix};

/// Where the mobile ☰ button sits on a screen.
pub(crate) fn hamburger_center(scr: Rect, safe_area: f32) -> Pos2 {
    let m = vec2(scr.width() * safe_area, scr.height() * safe_area);
    pos2(scr.right() - m.x - 66.0, scr.top() + m.y + 66.0)
}

struct D<'a> {
    p: Painter,
    scr: Rect,
    safe: Rect,
    theme: &'a Theme,
    family: GlyphFamily,
    bindings: &'a Bindings,
}

impl FrontEnd {
    /// Draws the current screen (in Play and Attract also the HUD from `frame.ui`) and records
    /// the focusable rectangles for the next `update`. Call it inside the egui pass, after
    /// `update`.
    pub fn draw(&mut self, ctx: &egui::Context, frame: Option<&PlayFrame>, env: &HudEnv) {
        let theme = env.theme;
        let scr = hud::screen(ctx);
        let d = D {
            p: hud::painter(ctx),
            scr,
            safe: hud::safe(scr, theme),
            theme,
            family: self.family(env.family),
            bindings: env.bindings,
        };
        self.hits.clear();
        self.hits_mode = Some(self.mode);
        self.hamburger = None;
        self.layout_actions = env.bindings.actions.len();
        let settings = self.settings.clone();
        let henv = HudEnv { family: d.family, settings: &settings, ..*env };
        match self.mode {
            Mode::Boot => self.draw_boot(&d),
            Mode::Title => self.draw_title(&d),
            Mode::Menu => self.draw_menu(&d),
            Mode::Chapters => self.draw_chapters(&d),
            Mode::Settings | Mode::PauseSettings => {
                if self.mode == Mode::PauseSettings {
                    self.draw_paused_backdrop(&d, frame, &henv);
                }
                self.draw_settings(&d)
            }
            Mode::Credits => self.draw_credits(&d),
            Mode::Confirm => {
                // The screen underneath, without its hit boxes.
                let back = self.confirm.map(|c| c.back).unwrap_or(Mode::Menu);
                self.suppress_hints = true;
                match back {
                    Mode::Paused => {
                        self.draw_paused_backdrop(&d, frame, &henv);
                        self.draw_pause(&d, frame);
                    }
                    _ => self.draw_menu(&d),
                }
                self.suppress_hints = false;
                self.hits.clear();
                self.draw_confirm(&d);
            }
            Mode::Attract | Mode::AttractCard | Mode::MobileMenu | Mode::MobileAbout => self.draw_attract(&d, frame, &henv),
            Mode::Play => {
                if let Some(f) = frame {
                    hud::draw_on(&d.p, d.scr, &f.ui, &henv);
                }
            }
            Mode::Paused => {
                self.draw_paused_backdrop(&d, frame, &henv);
                self.draw_pause(&d, frame);
            }
            Mode::PauseMap => {
                self.draw_paused_backdrop(&d, frame, &henv);
                self.draw_pause_map(&d, frame);
            }
            Mode::PauseObjectives => {
                self.draw_paused_backdrop(&d, frame, &henv);
                self.draw_objectives(&d, frame);
            }
        }
        // Transitions.
        if let Some(t) = self.fade_out {
            d.p.rect_filled(d.scr, 0.0, alpha(Color32::BLACK, (t / ATTRACT_FADE).clamp(0.0, 1.0)));
        }
        if self.title_fade > 0.0 {
            d.p.rect_filled(d.scr, 0.0, alpha(Color32::BLACK, (self.title_fade / TITLE_FADE).clamp(0.0, 1.0)));
        }
    }

    fn hit(&mut self, rect: Rect, target: Target) {
        self.hits.push(Hit { rect, target });
    }

    fn pulse(&self, period: f32) -> f32 {
        0.5 + 0.5 * (self.clock / period.max(0.1) * TAU).sin()
    }

    fn hints(&self, d: &D, list: &[(Nav, &str)]) {
        if self.suppress_hints {
            return;
        }
        hud::hint_bar(&d.p, d.scr, d.safe, &nav_hints(list, d.family), d.family, d.theme);
    }

    // ---------------------------------------------------------------- boot and title

    fn draw_boot(&mut self, d: &D) {
        d.p.rect_filled(d.scr, 0.0, Color32::from_rgb(12, 9, 6));
        paint::medallion(&d.p, d.scr.center() - vec2(0.0, 40.0), 90.0, d.theme);
        let dots = ((self.clock * 3.0) as usize % 4).min(3);
        d.p.text(
            d.scr.center() + vec2(0.0, 110.0),
            Align2::CENTER_CENTER,
            format!("Loading{}", ".".repeat(dots)),
            d.theme.body(32.0),
            d.theme.parchment,
        );
    }

    /// Dark gradients that keep text legible over the live scene.
    fn scene_shade(&self, d: &D, left: bool) {
        let t = d.theme;
        let s = d.scr;
        paint::vgradient(
            &d.p,
            Rect::from_min_max(s.min, pos2(s.right(), s.top() + s.height() * 0.3)),
            alpha(t.ink, 0.6),
            Color32::TRANSPARENT,
        );
        paint::vgradient(
            &d.p,
            Rect::from_min_max(pos2(s.left(), s.top() + s.height() * 0.62), s.max),
            Color32::TRANSPARENT,
            alpha(t.ink, 0.8),
        );
        if left {
            let w = s.width() * 0.55;
            paint::hgradient(
                &d.p,
                Rect::from_min_max(s.min, pos2(s.left() + w * 0.55, s.bottom())),
                alpha(t.ink, 0.88),
                alpha(t.ink, 0.7),
            );
            paint::hgradient(
                &d.p,
                Rect::from_min_max(pos2(s.left() + w * 0.55, s.top()), pos2(s.left() + w, s.bottom())),
                alpha(t.ink, 0.7),
                Color32::TRANSPARENT,
            );
        }
    }

    fn draw_title(&mut self, d: &D) {
        let t = d.theme;
        self.scene_shade(d, false);
        paint::vignette(&d.p, d.scr, 0.7, alpha(Color32::BLACK, 0.45));
        let c = pos2(d.scr.center().x, d.scr.top() + d.scr.height() * 0.40);
        paint::logo(&d.p, c, 1.0, t, &self.info.title, &self.info.tagline, LogoStyle::Title, 1.0);
        // Press ✕ to start.
        let a = 0.55 + 0.45 * self.pulse(t.pulse_seconds);
        let y = d.scr.top() + d.scr.height() * 0.78;
        let band = Rect::from_center_size(pos2(d.scr.center().x, y), vec2(900.0, 90.0));
        paint::hgradient(
            &d.p,
            Rect::from_min_max(band.min, pos2(band.center().x, band.bottom())),
            Color32::TRANSPARENT,
            alpha(t.ink, 0.65),
        );
        paint::hgradient(
            &d.p,
            Rect::from_min_max(pos2(band.center().x, band.top()), band.max),
            alpha(t.ink, 0.65),
            Color32::TRANSPARENT,
        );
        let pieces = if d.family == GlyphFamily::Keyboard {
            vec![Piece::Text("Press any key to start".into())]
        } else {
            vec![Piece::Text("Press".into()), Piece::Glyph(Glyph::Pad(PadButton::South)), Piece::Text("to start".into())]
        };
        glyph::paint_row(
            &d.p,
            pos2(d.scr.center().x, y),
            Align2::CENTER_CENTER,
            &pieces,
            t.bold(44.0),
            alpha(t.parchment_light, a),
            58.0,
            d.family,
            t,
        );
        if self.audio_locked {
            let y2 = y + 74.0;
            let r = d.p.text(
                pos2(d.scr.center().x + 24.0, y2),
                Align2::CENTER_CENTER,
                "Press any key or click to enable sound",
                t.body(28.0),
                alpha(t.parchment, 0.9),
            );
            paint::speaker(&d.p, pos2(r.left() - 30.0, y2), 34.0, alpha(t.parchment, 0.9), false);
        }
        if !self.info.id.is_empty() {
            d.p.text(
                pos2(d.safe.left(), d.safe.bottom()),
                Align2::LEFT_BOTTOM,
                &self.info.id,
                t.body(24.0),
                alpha(t.parchment, 0.6),
            );
        }
        self.hints(d, &[(Nav::Confirm, "Start")]);
    }

    // ---------------------------------------------------------------- menus

    /// A big vertical list over the scene; returns the list's bottom.
    fn list(&mut self, d: &D, top: f32, labels: &[&str], focus: usize, size: f32) -> f32 {
        let t = d.theme;
        let x0 = d.safe.left() + 36.0;
        let spacing = size * 1.52;
        let mut y = top;
        let w = (d.scr.width() * 0.36).max(640.0);
        for (i, label) in labels.iter().enumerate() {
            let focused = i == focus;
            let rect = Rect::from_min_size(pos2(x0 - 26.0, y - spacing * 0.46), vec2(w, spacing * 0.92));
            self.hit(rect, Target::Item(i));
            if focused {
                let k = (0.75 + 0.25 * (self.focus_age / 0.12).min(1.0)) * (0.9 + 0.1 * self.pulse(1.8));
                paint::focus_band(&d.p, rect, t, k, true);
            }
            let fs = quantize(if focused { size * t.focus_scale } else { size });
            let col = if focused { t.parchment_light } else { alpha(t.parchment, 0.78) };
            paint::text_outlined(&d.p, pos2(x0 + 6.0, y), Align2::LEFT_CENTER, label, t.display(fs), col, alpha(t.ink, 0.9), 3.0);
            if focused {
                paint::diamond(&d.p, pos2(rect.right() - 36.0, y), 22.0, t.amber_bright, Stroke::new(2.0, t.ink));
            }
            y += spacing;
        }
        y - spacing * 0.5
    }

    fn draw_menu(&mut self, d: &D) {
        let t = d.theme;
        self.scene_shade(d, true);
        paint::logo(&d.p, d.safe.min + vec2(10.0, 0.0), 1.2, t, &self.info.title, "", LogoStyle::Corner, 1.0);
        let items = self.menu_items();
        let labels: Vec<&str> = items.iter().map(|m| m.label()).collect();
        let focus = self.menu_focus.min(items.len() - 1);
        let top = d.safe.top() + 250.0 + (7 - items.len().min(7)) as f32 * 20.0;
        let bottom = self.list(d, top, &labels, focus, t.sizes.item);
        let desc = items.get(focus).map(|m| m.description()).unwrap_or("");
        description(d, pos2(d.safe.left() + 36.0, bottom + 34.0), (d.scr.width() * 0.36).max(640.0) - 30.0, desc);
        let mut hints = vec![(Nav::Confirm, "Select"), (Nav::Back, "Title")];
        if items[focus] == MenuItem::Quit {
            hints[0].1 = "Quit";
        }
        self.hints(d, &hints);
    }

    fn draw_chapters(&mut self, d: &D) {
        let t = d.theme;
        self.scene_shade(d, true);
        paint::text_outlined(
            &d.p,
            d.safe.min + vec2(36.0, 0.0),
            Align2::LEFT_TOP,
            "Replay a Round",
            t.display(t.sizes.title),
            t.parchment_light,
            alpha(t.ink, 0.9),
            4.0,
        );
        let owned: Vec<String> = self.info.chapters.iter().map(|(l, _)| l.clone()).collect();
        let labels: Vec<&str> = if owned.is_empty() { vec!["No Rounds yet"] } else { owned.iter().map(|s| s.as_str()).collect() };
        let n = labels.len();
        // At most 7 rows on screen: a window that keeps the focus visible.
        let start = self.chapter_focus.saturating_sub(6).min(n.saturating_sub(7));
        let shown: Vec<&str> = labels.iter().skip(start).take(7).copied().collect();
        let top = d.safe.top() + 200.0;
        let bottom = self.list(d, top, &shown, self.chapter_focus - start, t.sizes.item * 0.92);
        for h in &mut self.hits {
            if let Target::Item(i) = h.target {
                h.target = Target::Item(i + start);
            }
        }
        description(
            d,
            pos2(d.safe.left() + 36.0, bottom + 34.0),
            (d.scr.width() * 0.36).max(640.0) - 30.0,
            "Sail this Round again with your upgrades. Tides and patrols change each time.",
        );
        self.hints(d, &[(Nav::Confirm, "Set Sail"), (Nav::Back, "Back")]);
    }

    // ---------------------------------------------------------------- settings

    fn settings_panel(d: &D) -> Rect {
        Rect::from_min_max(pos2(d.safe.left(), d.safe.top() + 178.0), pos2(d.safe.right(), d.safe.bottom() - 128.0))
    }

    fn draw_settings(&mut self, d: &D) {
        let t = d.theme;
        if self.mode == Mode::Settings {
            self.scene_shade(d, false);
            d.p.rect_filled(d.scr, 0.0, alpha(t.ink, 0.55));
        }
        paint::text_outlined(
            &d.p,
            d.safe.min,
            Align2::LEFT_TOP,
            "Settings",
            t.display(t.sizes.title),
            t.parchment_light,
            alpha(t.ink, 0.9),
            4.0,
        );
        let panel = Self::settings_panel(d);
        // Tabs.
        let ty = d.safe.top() + 130.0;
        let gl = glyph::nav_glyph(Nav::TabPrev, d.family);
        let gr = glyph::nav_glyph(Nav::TabNext, d.family);
        glyph::paint(&d.p, pos2(panel.left() + 40.0, ty), 44.0, &gl, d.family, t);
        glyph::paint(&d.p, pos2(panel.right() - 40.0, ty), 44.0, &gr, d.family, t);
        let tabs_l = panel.left() + 100.0;
        let tw = (panel.width() - 200.0) / TABS.len() as f32;
        for (i, (name, _)) in TABS.iter().enumerate() {
            let r = Rect::from_center_size(pos2(tabs_l + tw * (i as f32 + 0.5), ty), vec2(tw - 12.0, 62.0));
            self.hit(r, Target::Tab(i));
            if i == self.tab {
                paint::glow(&d.p, r, 10.0, 18.0, alpha(t.amber_bright, 0.4));
                d.p.rect_filled(r, 10.0, t.parchment);
                d.p.rect_filled(
                    Rect::from_min_size(pos2(r.left() + 14.0, r.bottom() - 9.0), vec2(r.width() - 28.0, 5.0)),
                    2.0,
                    t.amber,
                );
                d.p.text(r.center() - vec2(0.0, 2.0), Align2::CENTER_CENTER, *name, t.bold(30.0), t.ink);
            } else {
                d.p.text(r.center(), Align2::CENTER_CENTER, *name, t.bold(28.0), alpha(t.parchment, 0.75));
            }
        }
        paint::panel(&d.p, panel, t, 0x5e7 + self.tab as u32);
        if let Some(page) = self.layout_page {
            self.draw_layout(d, panel, page);
            return;
        }
        let inner = panel.shrink2(vec2(64.0, 46.0));
        let rows = self.rows();
        let rh = (inner.height() / 7.0).min(84.0);
        for (i, row) in rows.iter().enumerate() {
            let r = Rect::from_min_size(pos2(inner.left(), inner.top() + i as f32 * rh), vec2(inner.width(), rh - 6.0));
            let focused = i == self.row;
            self.hit(r, Target::Item(i));
            if focused {
                paint::focus_band(&d.p, r, t, 0.8 + 0.2 * (self.focus_age / 0.12).min(1.0), false);
            }
            let fs = quantize(if focused { 34.0 * 1.06 } else { 34.0 });
            let label_x = r.left() + 30.0;
            match row {
                Row::Defaults => {
                    reset_icon(&d.p, pos2(label_x + 14.0, r.center().y), 30.0, t.ink);
                    d.p.text(pos2(label_x + 46.0, r.center().y), Align2::LEFT_CENTER, row.label(), t.bold(fs), t.ink);
                }
                Row::ButtonLayout => {
                    d.p.text(pos2(label_x, r.center().y), Align2::LEFT_CENTER, row.label(), t.bold(fs), t.ink);
                    paint::chevron(&d.p, pos2(r.right() - 40.0, r.center().y), 30.0, std::f32::consts::FRAC_PI_2, t.ink, 4.0);
                }
                _ => {
                    d.p.text(pos2(label_x, r.center().y), Align2::LEFT_CENTER, row.label(), t.bold(fs), t.ink);
                }
            }
            let va = Rect::from_min_max(pos2(r.left() + r.width() * 0.5, r.top()), pos2(r.right() - 16.0, r.bottom()));
            let arrow_col = if focused { t.amber } else { alpha(t.ink_soft, 0.55) };
            let left_c = pos2(va.left() + 24.0, va.center().y);
            match row.value(&self.settings) {
                RowValue::Choice(text) => {
                    let right_c = pos2(va.right() - 24.0, va.center().y);
                    self.picker(d, left_c, right_c, i, arrow_col);
                    d.p.text(pos2((left_c.x + right_c.x) * 0.5, va.center().y), Align2::CENTER_CENTER, text, t.bold(32.0), t.ink);
                }
                RowValue::Toggle(on) => {
                    let right_c = pos2(va.right() - 24.0, va.center().y);
                    self.picker(d, left_c, right_c, i, arrow_col);
                    let mid = pos2((left_c.x + right_c.x) * 0.5, va.center().y);
                    let lamp = pos2(mid.x - 52.0, mid.y);
                    d.p.circle_filled(lamp, 11.0, if on { t.green } else { alpha(t.ink_soft, 0.35) });
                    d.p.circle_stroke(lamp, 11.0, Stroke::new(2.0, t.ink));
                    d.p.text(
                        pos2(mid.x - 30.0, mid.y),
                        Align2::LEFT_CENTER,
                        if on { "On" } else { "Off" },
                        t.bold(32.0),
                        if on { t.ink } else { t.ink_soft },
                    );
                }
                RowValue::Slider(frac, text) => {
                    let right_c = pos2(va.right() - 150.0, va.center().y);
                    self.picker(d, left_c, right_c, i, arrow_col);
                    let track = Rect::from_min_max(
                        pos2(left_c.x + 34.0, va.center().y - 7.0),
                        pos2(right_c.x - 34.0, va.center().y + 7.0),
                    );
                    d.p.rect_filled(track.expand(2.0), 7.0, alpha(t.ink, 0.75));
                    let fill = Rect::from_min_size(track.min, vec2(track.width() * frac.clamp(0.0, 1.0), track.height()));
                    d.p.rect_filled(fill, 6.0, if focused { t.amber_bright } else { t.amber });
                    for k in 1..10 {
                        let x = track.left() + track.width() * k as f32 / 10.0;
                        d.p.vline(x, (track.top() + 3.0)..=(track.bottom() - 3.0), Stroke::new(1.0, alpha(t.ink, 0.35)));
                    }
                    let knob = pos2(fill.right(), track.center().y);
                    d.p.circle_filled(knob + vec2(0.0, 2.0), 15.0, alpha(Color32::BLACK, 0.35));
                    d.p.circle_filled(knob, 14.0, t.parchment_light);
                    d.p.circle_stroke(knob, 14.0, Stroke::new(3.0, t.ink));
                    d.p.text(pos2(va.right() - 10.0, va.center().y), Align2::RIGHT_CENTER, text, t.bold(32.0), t.ink);
                }
                RowValue::Action => {}
            }
            if i + 1 < rows.len() && !focused && i + 1 != self.row {
                d.p.hline((r.left() + 20.0)..=(r.right() - 20.0), r.bottom() + 3.0, Stroke::new(1.0, alpha(t.rope_dark, 0.25)));
            }
        }
        let desc = rows.get(self.row).map(|r| r.description()).unwrap_or("");
        description(d, pos2(panel.left() + 10.0, panel.bottom() + 40.0), panel.width() - 20.0, desc);
        let row = rows.get(self.row).copied().unwrap_or(Row::Defaults);
        let mut hints = Vec::new();
        match row.value(&self.settings) {
            RowValue::Action => hints.push((Nav::Confirm, "Select")),
            _ => hints.push((Nav::Left, "Change")),
        }
        hints.push((Nav::TabNext, "Tabs"));
        if row != Row::Defaults && row != Row::ButtonLayout {
            hints.push((Nav::Context, "Reset"));
        }
        hints.push((Nav::Back, "Back"));
        self.hints(d, &hints);
    }

    /// ◀ ▶ arrows of a row, with their hit boxes.
    fn picker(&mut self, d: &D, left: Pos2, right: Pos2, row: usize, col: Color32) {
        paint::arrow_head(&d.p, left, 28.0, -std::f32::consts::FRAC_PI_2, col);
        paint::arrow_head(&d.p, right, 28.0, std::f32::consts::FRAC_PI_2, col);
        self.hit(Rect::from_center_size(left, Vec2::splat(56.0)), Target::Left(row));
        self.hit(Rect::from_center_size(right, Vec2::splat(56.0)), Target::Right(row));
    }

    fn layout_entries(&self, d: &D) -> Vec<(Vec<Glyph>, String)> {
        let kb = d.family == GlyphFamily::Keyboard;
        let mut v: Vec<(Vec<Glyph>, String)> = vec![
            (if kb { vec![Glyph::key("WASD")] } else { vec![Glyph::LeftStick] }, "Move · Steer · Sails".into()),
            (if kb { vec![Glyph::MouseMove] } else { vec![Glyph::RightStick] }, "Camera · Choose a broadside".into()),
            (vec![glyph::nav_glyph(Nav::Start, d.family)], "Pause".into()),
        ];
        for a in &d.bindings.actions {
            v.push((vec![glyph::glyph_for_action(d.bindings, &a.name, d.family)], a.label.clone()));
        }
        v
    }

    fn draw_layout(&mut self, d: &D, panel: Rect, page: usize) {
        let t = d.theme;
        let entries = self.layout_entries(d);
        let pages = entries.len().div_ceil(6).max(1);
        let page = page.min(pages - 1);
        let inner = panel.shrink2(vec2(70.0, 50.0));
        d.p.text(inner.left_top(), Align2::LEFT_TOP, "Button Layout", t.display(52.0), t.ink);
        let col_w = inner.width() * 0.5;
        for (k, (glyphs, label)) in entries.iter().skip(page * 6).take(6).enumerate() {
            let (cx, cy) = (inner.left() + (k % 2) as f32 * col_w, inner.top() + 110.0 + (k / 2) as f32 * 120.0);
            let mut x = cx + 40.0;
            for g in glyphs {
                let s = glyph::size(&d.p, g, 64.0, d.family, t);
                glyph::paint(&d.p, pos2(x + s.x * 0.5, cy + 30.0), 64.0, g, d.family, t);
                x += s.x + 10.0;
            }
            paint::text_wrapped(&d.p, pos2(cx + 150.0, cy + 10.0), col_w - 180.0, label, t.bold(32.0), t.ink);
        }
        self.pager(d, pos2(panel.center().x, panel.bottom() - 54.0), page, pages);
        description(
            d,
            pos2(panel.left() + 10.0, panel.bottom() + 40.0),
            panel.width() - 20.0,
            "Menus: D-pad or stick moves, ✕ selects, ◯ goes back. Bindings live in bindings.toml.",
        );
        self.hints(d, &[(Nav::Left, "Page"), (Nav::Back, "Back")]);
    }

    /// ◀ Page n / m ▶ (the focused element of paged screens).
    fn pager(&mut self, d: &D, c: Pos2, page: usize, pages: usize) {
        let t = d.theme;
        let r = Rect::from_center_size(c, vec2(380.0, 70.0));
        paint::glow(&d.p, r, 30.0, 22.0, alpha(t.amber_bright, 0.45 + 0.15 * self.pulse(1.8)));
        d.p.rect_filled(r, 35.0, alpha(t.amber, 0.35));
        d.p.rect_stroke(r, 35.0, Stroke::new(3.0, t.amber), egui::StrokeKind::Inside);
        d.p.text(c, Align2::CENTER_CENTER, format!("Page {} / {}", page + 1, pages), t.bold(32.0), t.ink);
        let l = pos2(r.left() + 36.0, c.y);
        let rr = pos2(r.right() - 36.0, c.y);
        paint::arrow_head(&d.p, l, 30.0, -std::f32::consts::FRAC_PI_2, if pages > 1 { t.ink } else { alpha(t.ink, 0.3) });
        paint::arrow_head(&d.p, rr, 30.0, std::f32::consts::FRAC_PI_2, if pages > 1 { t.ink } else { alpha(t.ink, 0.3) });
        self.hit(Rect::from_center_size(l, Vec2::splat(70.0)), Target::PagePrev);
        self.hit(Rect::from_center_size(rr, Vec2::splat(70.0)), Target::PageNext);
    }

    // ---------------------------------------------------------------- credits

    fn draw_credits(&mut self, d: &D) {
        let t = d.theme;
        self.scene_shade(d, false);
        d.p.rect_filled(d.scr, 0.0, alpha(t.ink, 0.55));
        paint::text_outlined(
            &d.p,
            d.safe.min,
            Align2::LEFT_TOP,
            "Credits & About",
            t.display(t.sizes.title),
            t.parchment_light,
            alpha(t.ink, 0.9),
            4.0,
        );
        let panel = Rect::from_min_max(pos2(d.safe.left(), d.safe.top() + 120.0), pos2(d.safe.right(), d.safe.bottom() - 90.0));
        paint::panel(&d.p, panel, t, 0xc4ed);
        let inner = panel.shrink2(vec2(90.0, 60.0));
        let pages = measure_pages(d, &self.info.credits, inner.width(), inner.height() - 120.0, 48.0, 30.0);
        self.credit_layout = Some(pages.clone());
        let page = self.credits_page.min(pages.len().saturating_sub(1));
        let mut y = inner.top();
        for &i in pages.get(page).map(|v| v.as_slice()).unwrap_or(&[]) {
            let (h, b) = &self.info.credits[i];
            let hr = d.p.text(pos2(inner.left(), y), Align2::LEFT_TOP, h, t.display(48.0), t.ink);
            d.p.hline(
                inner.left()..=(inner.left() + hr.width().max(120.0)),
                hr.bottom() + 2.0,
                Stroke::new(2.0, alpha(t.amber, 0.8)),
            );
            let br =
                paint::text_wrapped(&d.p, pos2(inner.left(), hr.bottom() + 14.0), inner.width(), b, t.body(30.0), t.ink_soft);
            y = br.bottom() + 30.0;
        }
        self.pager(d, pos2(panel.center().x, panel.bottom() - 60.0), page, pages.len().max(1));
        self.hints(d, &[(Nav::Left, "Pages"), (Nav::Back, "Back")]);
    }

    // ---------------------------------------------------------------- confirm

    fn draw_confirm(&mut self, d: &D) {
        let t = d.theme;
        let Some(c) = self.confirm else { return };
        d.p.rect_filled(d.scr, 0.0, alpha(Color32::BLACK, 0.6));
        let panel = Rect::from_center_size(d.scr.center(), vec2(1060.0, 500.0));
        paint::panel(&d.p, panel, t, 0xc0f);
        let (q, detail, safe, risky) = c.kind.text();
        d.p.text(pos2(panel.center().x, panel.top() + 70.0), Align2::CENTER_TOP, q, t.display(64.0), t.ink);
        d.p.text(pos2(panel.center().x, panel.top() + 170.0), Align2::CENTER_TOP, detail, t.body(32.0), t.ink_soft);
        let by = panel.bottom() - 120.0;
        for (i, label) in [safe, risky].iter().enumerate() {
            let focused = c.focus == i;
            let x = panel.center().x + if i == 0 { -230.0 } else { 230.0 };
            let r = Rect::from_center_size(pos2(x, by), vec2(400.0, 104.0) * if focused { d.theme.focus_scale } else { 1.0 });
            self.hit(r, Target::Item(i));
            hud::button(&d.p, r, label, focused, t, i as u32);
            if i == 1 && c.kind != ConfirmKind::Quit {
                // Mark the destructive choice.
                if !focused {
                    d.p.circle_filled(pos2(r.left() + 30.0, r.center().y), 6.0, t.red);
                }
            }
        }
        self.hints(d, &[(Nav::Confirm, "Select"), (Nav::Back, "Cancel")]);
    }

    // ---------------------------------------------------------------- pause

    fn draw_paused_backdrop(&self, d: &D, frame: Option<&PlayFrame>, env: &HudEnv) {
        let _ = (frame, env);
        d.p.rect_filled(d.scr, 0.0, alpha(Color32::BLACK, 0.5));
        paint::vignette(&d.p, d.scr, 0.45, alpha(Color32::BLACK, 0.55));
    }

    fn draw_pause(&mut self, d: &D, frame: Option<&PlayFrame>) {
        let t = d.theme;
        let panel = Rect::from_min_size(d.safe.min, vec2(720.0, 230.0 + PAUSE_ITEMS.len() as f32 * 82.0 + 40.0));
        paint::panel(&d.p, panel, t, 0x9a5);
        d.p.text(pos2(panel.left() + 70.0, panel.top() + 52.0), Align2::LEFT_TOP, "Paused", t.display(t.sizes.title), t.ink);
        let obj = frame.and_then(|f| f.ui.objective.as_ref());
        if let Some(o) = obj {
            paint::text_spaced(
                &d.p,
                pos2(panel.left() + 74.0, panel.top() + 140.0),
                Align2::LEFT_TOP,
                &o.title.to_uppercase(),
                t.bold(24.0),
                t.ink_soft,
                1.2,
                None,
            );
        }
        let x0 = panel.left() + 76.0;
        let spacing = 82.0;
        let mut y = panel.top() + 230.0;
        for (i, item) in PAUSE_ITEMS.iter().enumerate() {
            let focused = i == self.pause_focus;
            let r = Rect::from_min_size(pos2(x0 - 24.0, y - spacing * 0.45), vec2(panel.width() - 110.0, spacing * 0.9));
            self.hit(r, Target::Item(i));
            if focused {
                paint::focus_band(&d.p, r, t, 0.8 + 0.2 * (self.focus_age / 0.12).min(1.0), false);
            }
            let fs = quantize(if focused { 46.0 * t.focus_scale } else { 46.0 });
            d.p.text(pos2(x0, y), Align2::LEFT_CENTER, item.label(), t.display(fs), t.ink);
            y += spacing;
        }
        // The current objective, on the right.
        if let Some(o) = obj {
            let card = Rect::from_min_size(
                pos2(panel.right() + 70.0, d.safe.top() + 40.0),
                vec2((d.safe.right() - panel.right() - 70.0).min(820.0), 300.0),
            );
            objective_card(d, card, o);
        }
        self.hints(d, &[(Nav::Confirm, "Select"), (Nav::Map, "Map"), (Nav::Back, "Resume")]);
    }

    fn draw_pause_map(&mut self, d: &D, frame: Option<&PlayFrame>) {
        let ui = frame.map(|f| &f.ui);
        let area = Rect::from_min_max(d.safe.min, pos2(d.safe.right(), d.safe.bottom() - 84.0));
        map::draw(
            &d.p,
            area,
            ui.and_then(|u| u.map.as_ref()),
            ui.map(|u| u.map_icons.as_slice()).unwrap_or(&[]),
            &self.map_view,
            d.theme,
            None,
        );
        let hints = hud::map_hints(d.family, self.map_view.legend, "Back");
        hud::hint_bar(&d.p, d.scr, d.safe, &hints, d.family, d.theme);
    }

    fn draw_objectives(&mut self, d: &D, frame: Option<&PlayFrame>) {
        let t = d.theme;
        let rows = frame.and_then(|f| f.ui.objective.as_ref()).map(|o| o.optional.len().min(6)).unwrap_or(0);
        let h =
            if rows == 0 && frame.and_then(|f| f.ui.objective.as_ref()).is_none() { 420.0 } else { 560.0 + rows as f32 * 56.0 };
        let panel = Rect::from_center_size(d.scr.center() - vec2(0.0, 30.0), vec2(1240.0, h));
        paint::panel(&d.p, panel, t, 0x0b1);
        let inner = panel.shrink2(vec2(90.0, 70.0));
        let obj = frame.and_then(|f| f.ui.objective.as_ref());
        let Some(o) = obj else {
            d.p.text(panel.center(), Align2::CENTER_CENTER, "No orders yet. Enjoy the sea air.", t.display(52.0), t.ink_soft);
            self.hints(d, &[(Nav::Back, "Back")]);
            return;
        };
        let tr = d.p.text(inner.left_top(), Align2::LEFT_TOP, &o.title, t.display(60.0), t.ink);
        d.p.hline(inner.left()..=inner.right(), tr.bottom() + 8.0, Stroke::new(2.0, alpha(t.rope_dark, 0.5)));
        let y = tr.bottom() + 40.0;
        paint::text_spaced(&d.p, pos2(inner.left(), y), Align2::LEFT_TOP, "NOW", t.bold(26.0), t.amber, 2.0, None);
        paint::diamond(&d.p, pos2(inner.left() + 16.0, y + 66.0), 26.0, t.gold, Stroke::new(2.0, t.ink));
        let sr =
            paint::text_wrapped(&d.p, pos2(inner.left() + 48.0, y + 44.0), inner.width() - 48.0, &o.text, t.bold(40.0), t.ink);
        let mut y = sr.bottom() + 50.0;
        d.p.text(pos2(inner.left(), y), Align2::LEFT_TOP, "Full Pour", t.display(50.0), t.ink);
        let done = o.optional.iter().filter(|(_, d)| *d).count();
        d.p.text(
            pos2(inner.right(), y + 12.0),
            Align2::RIGHT_TOP,
            format!("{done} / {}", o.optional.len()),
            t.bold(32.0),
            t.ink_soft,
        );
        y += 76.0;
        for (text, ok) in o.optional.iter().take(6) {
            let bx = Rect::from_min_size(pos2(inner.left() + 4.0, y), Vec2::splat(34.0));
            d.p.rect_stroke(bx, 4.0, Stroke::new(3.0, t.ink), egui::StrokeKind::Inside);
            if *ok {
                paint::tick(&d.p, bx.center() + vec2(3.0, -3.0), 44.0, t.green);
            }
            d.p.text(
                pos2(inner.left() + 56.0, bx.center().y),
                Align2::LEFT_CENTER,
                text,
                t.body(32.0),
                if *ok { t.ink } else { t.ink_soft },
            );
            y += 56.0;
        }
        self.hints(d, &[(Nav::Back, "Back")]);
    }

    // ---------------------------------------------------------------- attract

    fn draw_attract(&mut self, d: &D, frame: Option<&PlayFrame>, env: &HudEnv) {
        let t = d.theme;
        if self.mobile {
            self.draw_mobile(d);
            return;
        }
        if let Some(ct) = self.attract.card {
            self.draw_card(d, ct);
            return;
        }
        // Letterbox and framing; the game's HUD fits between the bars.
        let bar = 132.0;
        if let Some(f) = frame {
            if self.attract.loaded {
                let inner = d.scr.shrink2(vec2(0.0, bar - 10.0));
                hud::draw_on(&d.p, inner, &f.ui, env);
            }
        }
        let top = Rect::from_min_size(d.scr.min, vec2(d.scr.width(), bar));
        let bottom = Rect::from_min_max(pos2(d.scr.left(), d.scr.bottom() - bar), d.scr.max);
        d.p.rect_filled(top, 0.0, Color32::BLACK);
        d.p.rect_filled(bottom, 0.0, Color32::BLACK);
        // DEMO PLAY badge.
        let badge = Rect::from_min_size(pos2(d.safe.left(), d.safe.top() + 4.0), vec2(250.0, 62.0));
        d.p.rect_filled(badge, 8.0, t.red);
        d.p.rect_stroke(badge.shrink(4.0), 6.0, Stroke::new(2.0, alpha(t.parchment_light, 0.7)), egui::StrokeKind::Inside);
        let blink = if (self.clock * 1.25).fract() < 0.6 { 1.0 } else { 0.25 };
        d.p.circle_filled(pos2(badge.left() + 30.0, badge.center().y), 9.0, alpha(t.parchment_light, blink));
        paint::text_spaced(
            &d.p,
            pos2(badge.left() + 52.0, badge.center().y),
            Align2::LEFT_CENTER,
            "DEMO PLAY",
            t.bold(32.0),
            t.parchment_light,
            2.0,
            None,
        );
        // The logo in the top-right corner.
        let logo_w = 430.0;
        paint::logo(
            &d.p,
            pos2(d.safe.right() - logo_w, d.safe.top() - 14.0),
            0.8,
            t,
            &self.info.title,
            "",
            LogoStyle::Corner,
            0.95,
        );
        // Caption and PRESS ANY BUTTON.
        let seg = self.info.demo.get(self.attract.segment);
        let caption = frame
            .and_then(|f| f.ui.demo.as_ref())
            .map(|dm| dm.caption.clone())
            .filter(|c| !c.is_empty())
            .or_else(|| seg.map(|s| s.caption.clone()))
            .unwrap_or_default();
        let cy = d.scr.bottom() - bar * 0.5 + 4.0;
        if let Some(s) = seg {
            if s.seconds > 0.0 {
                let f = (self.attract.elapsed / s.seconds).clamp(0.0, 1.0);
                d.p.hline(
                    bottom.left()..=(bottom.left() + bottom.width() * f),
                    bottom.top() + 1.5,
                    Stroke::new(3.0, alpha(t.amber, 0.8)),
                );
            }
        }
        let a = 0.45 + 0.55 * self.pulse(t.pulse_seconds);
        paint::text_spaced(
            &d.p,
            pos2(d.safe.right(), cy),
            Align2::RIGHT_CENTER,
            "PRESS ANY BUTTON",
            t.bold(40.0),
            alpha(t.gold, a),
            3.0,
            None,
        );
        if !caption.is_empty() {
            let max_w = d.safe.width() - 520.0;
            let font = t.body(30.0);
            let mut cap = caption.clone();
            while paint::text_size(&d.p, &cap, font.clone()).x > max_w && cap.chars().count() > 8 {
                cap = cap.chars().take(cap.chars().count() - 2).collect::<String>();
                cap.push('…');
                cap = cap.replace("……", "…");
            }
            d.p.text(pos2(d.safe.left(), cy), Align2::LEFT_CENTER, cap, font, t.parchment_light);
        }
        // Fade in once the segment's scene is ready (black until then).
        let cover = if self.attract.loaded { 1.0 - (self.attract.shown / 0.6).clamp(0.0, 1.0) } else { 1.0 };
        if cover > 0.0 {
            d.p.rect_filled(d.scr, 0.0, alpha(Color32::BLACK, cover));
        }
    }

    fn draw_card(&mut self, d: &D, ct: f32) {
        let t = d.theme;
        d.p.rect_filled(d.scr, 0.0, Color32::from_rgb(14, 10, 7));
        paint::vignette(&d.p, d.scr, 0.3, alpha(Color32::BLACK, 0.8));
        paint::blob(&d.p, d.scr.center() - vec2(0.0, 60.0), 520.0, alpha(t.amber, 0.12));
        let fade = (ct / 0.4).clamp(0.0, 1.0).min(((CARD_SECONDS - ct) / 0.4).clamp(0.0, 1.0));
        let c = pos2(d.scr.center().x, d.scr.top() + d.scr.height() * 0.42);
        paint::logo(&d.p, c, 1.0, t, &self.info.title, &self.info.tagline, LogoStyle::Title, fade);
        let a = (0.45 + 0.55 * self.pulse(t.pulse_seconds)) * fade;
        paint::text_spaced(
            &d.p,
            pos2(d.scr.center().x, d.scr.top() + d.scr.height() * 0.8),
            Align2::CENTER_CENTER,
            "PRESS ANY BUTTON",
            t.bold(44.0),
            alpha(t.gold, a),
            4.0,
            None,
        );
    }

    // ---------------------------------------------------------------- mobile

    fn draw_mobile(&mut self, d: &D) {
        let t = d.theme;
        // A subtle logo and the ☰ button: nothing else.
        paint::medallion(&d.p, pos2(d.safe.left() + 44.0, d.safe.bottom() - 44.0), 40.0, t);
        let c = hamburger_center(d.scr, t.safe_area);
        self.hamburger = Some(c);
        let r = Rect::from_center_size(c, Vec2::splat(132.0));
        if matches!(self.mode, Mode::Attract | Mode::AttractCard) {
            d.p.rect_filled(r, 28.0, alpha(t.ink, 0.6));
            d.p.rect_stroke(r, 28.0, Stroke::new(2.5, alpha(t.parchment, 0.5)), egui::StrokeKind::Inside);
            paint::hamburger(&d.p, r.shrink(18.0), t.parchment_light);
            self.hit(r.expand(12.0), Target::Hamburger);
            return;
        }
        d.p.rect_filled(d.scr, 0.0, alpha(Color32::BLACK, 0.55));
        self.hit(d.scr, Target::Outside);
        if self.mode == Mode::MobileMenu {
            let panel = Rect::from_center_size(d.scr.center(), vec2(980.0, 860.0));
            self.hit(panel, Target::Panel);
            paint::panel(&d.p, panel, t, 0x30b);
            d.p.text(pos2(panel.center().x, panel.top() + 50.0), Align2::CENTER_TOP, "Assassin's Keg", t.display(64.0), t.ink);
            let rows = MOBILE_ITEMS;
            let rh = 150.0;
            let top = panel.top() + 170.0;
            for (i, item) in rows.iter().enumerate() {
                let rr =
                    Rect::from_min_size(pos2(panel.left() + 70.0, top + i as f32 * rh), vec2(panel.width() - 140.0, rh - 20.0));
                self.hit(rr, Target::Item(i));
                let focused = i == self.mobile_focus;
                d.p.rect_filled(rr, 18.0, if focused { alpha(t.amber, 0.35) } else { alpha(t.parchment_dark, 0.55) });
                d.p.rect_stroke(rr, 18.0, Stroke::new(3.0, alpha(t.rope_dark, 0.7)), egui::StrokeKind::Inside);
                let (label, value) = match item {
                    MobileItem::Sound => ("Sound", Some(if self.sound_on { "On" } else { "Off" }.to_string())),
                    MobileItem::Quality => ("Quality", Some(format!("{:?}", self.settings.display.quality))),
                    MobileItem::About => ("About", None),
                    MobileItem::Close => ("Close", None),
                };
                if *item == MobileItem::Sound {
                    paint::speaker(&d.p, pos2(rr.left() + 60.0, rr.center().y), 54.0, t.ink, self.sound_on);
                }
                let lx = if *item == MobileItem::Sound { rr.left() + 110.0 } else { rr.left() + 44.0 };
                d.p.text(pos2(lx, rr.center().y), Align2::LEFT_CENTER, label, t.bold(56.0), t.ink);
                if let Some(v) = value {
                    d.p.text(pos2(rr.right() - 44.0, rr.center().y), Align2::RIGHT_CENTER, v, t.bold(56.0), t.ink_soft);
                }
            }
        } else {
            let panel = Rect::from_center_size(d.scr.center(), vec2((d.scr.width() * 0.8).min(1700.0), 900.0));
            self.hit(panel, Target::Panel);
            paint::panel(&d.p, panel, t, 0xab0);
            let inner = panel.shrink2(vec2(90.0, 70.0));
            let pages = measure_pages(d, &self.info.credits, inner.width(), inner.height() - 110.0, 56.0, 40.0);
            self.about_layout = Some(pages.clone());
            let page = self.about_page.min(pages.len().saturating_sub(1));
            let mut y = inner.top();
            for &i in pages.get(page).map(|v| v.as_slice()).unwrap_or(&[]) {
                let (h, b) = &self.info.credits[i];
                let hr = d.p.text(pos2(inner.left(), y), Align2::LEFT_TOP, h, t.display(56.0), t.ink);
                let br =
                    paint::text_wrapped(&d.p, pos2(inner.left(), hr.bottom() + 14.0), inner.width(), b, t.body(40.0), t.ink_soft);
                y = br.bottom() + 26.0;
            }
            let by = panel.bottom() - 100.0;
            let close = Rect::from_center_size(pos2(panel.right() - 260.0, by), vec2(380.0, 120.0));
            hud::button_sized(&d.p, close, "Close", true, t, 52.0);
            self.hit(close, Target::Item(0));
            if pages.len() > 1 {
                let next = Rect::from_center_size(pos2(panel.left() + 260.0, by), vec2(380.0, 120.0));
                hud::button_sized(&d.p, next, &format!("Next {}/{}", page + 1, pages.len()), false, t, 52.0);
                self.hit(next, Target::PageNext);
            }
        }
    }
}

/// Splits (heading, body) blocks into pages that fit `height` at the given sizes.
fn measure_pages(d: &D, blocks: &[(String, String)], width: f32, height: f32, head: f32, body: f32) -> Vec<Vec<usize>> {
    let mut pages: Vec<Vec<usize>> = vec![Vec::new()];
    let mut y = 0.0;
    for (i, (h, b)) in blocks.iter().enumerate() {
        let hh = paint::text_size(&d.p, h, d.theme.display(head)).y;
        let bh = paint::wrapped_height(&d.p, b, d.theme.body(body), width);
        let block = hh + 14.0 + bh + 30.0;
        if y + block > height && !pages.last().is_some_and(|p| p.is_empty()) {
            pages.push(Vec::new());
            y = 0.0;
        }
        pages.last_mut().expect("page").push(i);
        y += block;
    }
    pages
}

/// A one-line description with a small diamond.
fn description(d: &D, pos: Pos2, width: f32, text: &str) {
    let t = d.theme;
    if text.is_empty() {
        return;
    }
    paint::diamond(&d.p, pos + vec2(10.0, 19.0), 16.0, t.amber_bright, Stroke::NONE);
    let font = t.body(30.0);
    let galley = d.p.layout(text.to_string(), font, t.parchment_light, width - 34.0);
    let gs = galley.size();
    d.p.galley_with_override_text_color(pos + vec2(36.0 + 2.0, 2.0), galley.clone(), alpha(Color32::BLACK, 0.7));
    d.p.galley(pos + vec2(36.0, 0.0), galley, t.parchment_light);
    let _ = gs;
}

fn reset_icon(p: &Painter, c: Pos2, s: f32, col: Color32) {
    let pts = paint::arc_points(c, s * 0.38, 0.5, 5.4, 20);
    let end = *pts.last().expect("arc");
    p.add(egui::epaint::PathShape::line(pts, egui::epaint::PathStroke::new(s * 0.1, col)));
    paint::arrow_head(p, end + vec2(s * 0.05, -s * 0.02), s * 0.36, 1.2, col);
}

/// A dark card with the current objective (pause menu).
fn objective_card(d: &D, card: Rect, o: &Objective) {
    let t = d.theme;
    let h = 150.0 + o.optional.len().min(5) as f32 * 50.0;
    let card = Rect::from_min_size(card.min, vec2(card.width(), h));
    paint::ink_panel(&d.p, card, t, 1.0);
    paint::text_spaced(
        &d.p,
        card.left_top() + vec2(32.0, 26.0),
        Align2::LEFT_TOP,
        "CURRENT OBJECTIVE",
        t.bold(24.0),
        t.gold,
        2.0,
        None,
    );
    paint::diamond(&d.p, card.left_top() + vec2(44.0, 92.0), 24.0, t.gold, Stroke::new(2.0, t.ink));
    paint::text_wrapped(&d.p, card.left_top() + vec2(72.0, 70.0), card.width() - 100.0, &o.text, t.bold(36.0), t.parchment_light);
    let mut y = card.top() + 140.0;
    for (text, ok) in o.optional.iter().take(5) {
        let bx = Rect::from_min_size(pos2(card.left() + 34.0, y), Vec2::splat(26.0));
        d.p.rect_stroke(bx, 3.0, Stroke::new(2.0, t.parchment), egui::StrokeKind::Inside);
        if *ok {
            paint::tick(&d.p, bx.center() + vec2(2.0, -2.0), 34.0, mix(t.green, Color32::WHITE, 0.3));
        }
        d.p.text(
            pos2(card.left() + 76.0, bx.center().y),
            Align2::LEFT_CENTER,
            text,
            t.body(28.0),
            if *ok { alpha(t.parchment, 0.7) } else { t.parchment_light },
        );
        y += 50.0;
    }
}
