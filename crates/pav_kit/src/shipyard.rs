//! Procedural ships: a hull lofted from cross-sections (keel, bilge, tumblehome, sheer, a
//! pointed bow and a flat transom) with painted wales and gunports, a deck and rails, tapered
//! masts, yards, a bowsprit, standing rigging and a figurehead; and sails as cloth grids
//! billowed by the wind and the sail level (square sails or fore-and-aft), with an emblem.
//! Local frame: x to starboard, y up from the waterline, z toward the bow.

use std::f32::consts::PI;

use glam::{Quat, Vec2, Vec3};
use pav_core::Color;
use pav_core::play::Mesh;
use serde::{Deserialize, Serialize};

use crate::naval::HullClass;

/// How a ship is painted and rigged (data).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShipStyle {
    pub hull: String,
    pub bottom: String,
    pub wale: String,
    pub strake: String,
    pub deck: String,
    pub rail: String,
    pub sail: String,
    pub emblem: String,
    /// "square" or "fore_and_aft".
    pub rig: String,
    pub figurehead: bool,
}

impl Default for ShipStyle {
    fn default() -> Self {
        Self {
            hull: "#3b2a20".into(),
            bottom: "#4a3a30".into(),
            wale: "#c9a24a".into(),
            strake: "#2a1f18".into(),
            deck: "#a07b52".into(),
            rail: "#4a3424".into(),
            sail: "#efe7d6".into(),
            emblem: "#2c3e66".into(),
            rig: "square".into(),
            figurehead: false,
        }
    }
}

fn lin(hex: &str) -> [f32; 3] {
    Color::hex(hex).0
}

fn shade(c: [f32; 3], k: f32) -> [f32; 3] {
    [c[0] * k, c[1] * k, c[2] * k]
}

/// Half-width of the hull at fraction `u` along it (0 = stern, 1 = bow), as a share of the
/// half beam: full amidships, a flat transom aft, a fine entry forward.
fn plan(u: f32) -> f32 {
    if u < 0.12 {
        0.72 + (u / 0.12) * 0.28
    } else if u < 0.6 {
        1.0
    } else {
        let t = (u - 0.6) / 0.4;
        (1.0 - t.powf(1.8)).max(0.0).sqrt() * 0.98 + 0.02
    }
}

/// Deck height above the waterline at `u` (sheer: higher at the ends).
fn deck_y(c: &HullClass, u: f32) -> f32 {
    let s = (u - 0.45).abs() / 0.55;
    c.freeboard + s * s * c.freeboard * 0.35
}

/// Section shape: the half-width at height fraction `h` (0 = keel, 1 = deck) as a share of
/// the station's half-width.
fn section(h: f32) -> f32 {
    if h < 0.35 { 0.18 + 0.82 * (h / 0.35).sqrt() } else { 1.0 - (h - 0.35) * 0.12 }
}

/// The hull: sides (both), transom, deck, rails, wales and gunports. `damage` 0..1 adds dark
/// shot holes (seeded by `seed`).
pub fn hull_mesh(c: &HullClass, style: &ShipStyle, id: u64, damage: f32, seed: u32) -> Mesh {
    let mut m = Mesh::new(id);
    let stations = 24usize;
    let rows = 10usize;
    let half = c.beam * 0.5;
    let keel = -c.draft;
    let (hull, bottom, wale, strake, deck, rail) =
        (lin(&style.hull), lin(&style.bottom), lin(&style.wale), lin(&style.strake), lin(&style.deck), lin(&style.rail));
    let z_of = |u: f32| (u - 0.5) * c.length;
    // Grid of points on the starboard side: [station][row].
    let mut pts = vec![vec![Vec3::ZERO; rows + 1]; stations + 1];
    for (i, row) in pts.iter_mut().enumerate() {
        let u = i as f32 / stations as f32;
        let w = plan(u) * half;
        let top = deck_y(c, u);
        // The keel rises toward the bow (a forefoot) and the stern (a run).
        let k = keel + (u - 0.5).abs().powf(3.0) * 4.0 * (-keel) * 0.6;
        for (j, p) in row.iter_mut().enumerate() {
            let h = j as f32 / rows as f32;
            let y = k + (top - k) * h;
            *p = Vec3::new(w * section(h), y, z_of(u));
        }
    }
    let gun_band = (c.freeboard * 0.45, c.freeboard * 0.72);
    let color_at = |p: Vec3, u: f32| -> [f32; 3] {
        if p.y < 0.25 {
            return bottom;
        }
        if p.y > gun_band.0 && p.y < gun_band.1 && (0.18..0.82).contains(&u) {
            return strake;
        }
        let band = (p.y - c.freeboard * 0.3).abs() < 0.12 || (p.y - c.freeboard * 0.85).abs() < 0.09;
        if band { wale } else { hull }
    };
    let mut rng = seed.wrapping_mul(2654435761).max(1);
    let mut rand = move || {
        rng ^= rng << 13;
        rng ^= rng >> 17;
        rng ^= rng << 5;
        (rng % 10000) as f32 / 10000.0
    };
    let holes: Vec<(f32, f32)> = (0..((damage * 14.0) as usize)).map(|_| (0.15 + rand() * 0.7, 0.35 + rand() * 0.5)).collect();
    for side in [1.0f32, -1.0] {
        let mirror = |p: Vec3| Vec3::new(p.x * side, p.y, p.z);
        for i in 0..stations {
            for j in 0..rows {
                let a = mirror(pts[i][j]);
                let b = mirror(pts[i + 1][j]);
                let cc = mirror(pts[i + 1][j + 1]);
                let d = mirror(pts[i][j + 1]);
                let u = (i as f32 + 0.5) / stations as f32;
                let h = (j as f32 + 0.5) / rows as f32;
                let mut col = color_at((a + cc) * 0.5, u);
                if holes.iter().any(|&(hu, hh)| (hu - u).abs() < 0.03 && (hh - h).abs() < 0.07) {
                    col = [0.02, 0.015, 0.01];
                }
                if side > 0.0 {
                    m.quad(a, d, cc, b, col);
                } else {
                    m.quad(a, b, cc, d, col);
                }
            }
        }
        // Gunports: dark squares with a lid line, on the strake.
        let n = c.guns_per_side.max(1);
        for g in 0..n {
            let u = 0.22 + (g as f32 + 0.5) / n as f32 * 0.56;
            let fi = u * stations as f32;
            let i = (fi as usize).min(stations - 1);
            let f = fi - i as f32;
            let h0 = gun_band.0 + 0.08;
            let h1 = gun_band.1 - 0.05;
            let at = |y: f32| {
                let lerp = |row: &Vec<Vec3>| {
                    let top = row[rows].y;
                    let bot = row[0].y;
                    let hh = ((y - bot) / (top - bot)).clamp(0.0, 1.0) * rows as f32;
                    let jj = (hh as usize).min(rows - 1);
                    row[jj].lerp(row[jj + 1], hh - jj as f32)
                };
                lerp(&pts[i]).lerp(lerp(&pts[i + 1]), f)
            };
            let pa = at(h0);
            let pb = at(h1);
            let half_w = (c.length / n as f32 * 0.17).min(0.45);
            let out = Vec3::X * 0.04;
            let p0 = mirror(pa + Vec3::new(0.0, 0.0, -half_w) + out);
            let p1 = mirror(pa + Vec3::new(0.0, 0.0, half_w) + out);
            let p2 = mirror(Vec3::new(pb.x, pb.y, pa.z + half_w) + out);
            let p3 = mirror(Vec3::new(pb.x, pb.y, pa.z - half_w) + out);
            if side > 0.0 {
                m.quad(p0, p3, p2, p1, [0.03, 0.025, 0.02]);
            } else {
                m.quad(p0, p1, p2, p3, [0.03, 0.025, 0.02]);
            }
        }
    }
    // Transom (flat stern) and the bow's stem (closing strip).
    for j in 0..rows {
        let a = pts[0][j];
        let b = pts[0][j + 1];
        let col = if a.y < 0.25 { bottom } else { shade(hull, 0.85) };
        m.quad(Vec3::new(a.x, a.y, a.z), Vec3::new(-a.x, a.y, a.z), Vec3::new(-b.x, b.y, b.z), Vec3::new(b.x, b.y, b.z), col);
    }
    // Stern windows (a band of warm light).
    let sy = deck_y(c, 0.0);
    let sz = -c.length * 0.5 - 0.03;
    let sw = half * 0.55;
    m.quad(
        Vec3::new(sw, sy - 0.9, sz),
        Vec3::new(-sw, sy - 0.9, sz),
        Vec3::new(-sw, sy - 0.5, sz),
        Vec3::new(sw, sy - 0.5, sz),
        [0.9, 0.62, 0.25],
    );
    // Deck.
    for i in 0..stations {
        let a = pts[i][rows];
        let b = pts[i + 1][rows];
        let plank = if i % 2 == 0 { deck } else { shade(deck, 0.9) };
        m.quad(
            Vec3::new(-a.x, a.y - 0.05, a.z),
            Vec3::new(-b.x, b.y - 0.05, b.z),
            Vec3::new(b.x, b.y - 0.05, b.z),
            Vec3::new(a.x, a.y - 0.05, a.z),
            plank,
        );
    }
    // Rails: a low bulwark along both sides with a cap.
    for side in [1.0f32, -1.0] {
        for i in 0..stations {
            let a = pts[i][rows];
            let b = pts[i + 1][rows];
            let (a, b) = (Vec3::new(a.x * side, a.y, a.z), Vec3::new(b.x * side, b.y, b.z));
            let up = Vec3::Y * 0.9;
            let inward = Vec3::new(-side * 0.12, 0.0, 0.0);
            if side > 0.0 {
                m.quad(a, a + up, b + up, b, rail);
                m.quad(a + up, a + up + inward, b + up + inward, b + up, shade(rail, 1.25));
            } else {
                m.quad(a, b, b + up, a + up, rail);
                m.quad(a + up, b + up, b + up + inward, a + up + inward, shade(rail, 1.25));
            }
        }
    }
    // Masts (tapered octagonal prisms), yards and the bowsprit.
    let masts = mast_positions(c);
    let wood = lin("#5c4330");
    for (k, &mz) in masts.iter().enumerate() {
        let h = c.mast_height * if k == 0 && masts.len() > 2 { 0.9 } else { 1.0 };
        let base = Vec3::new(0.0, deck_y(c, mz / c.length + 0.5), mz);
        cylinder(&mut m, base, base + Vec3::Y * h, 0.32, 0.12, wood);
        if style.rig == "square" {
            for (yi, frac) in [0.38f32, 0.68, 0.92].iter().enumerate() {
                let y = base.y + h * frac;
                let span = half * (2.2 - yi as f32 * 0.45);
                cylinder(&mut m, Vec3::new(-span, y, mz), Vec3::new(span, y, mz), 0.12, 0.08, wood);
            }
        } else {
            // A boom and a gaff for fore-and-aft sails.
            let y0 = base.y + 1.6;
            cylinder(&mut m, Vec3::new(0.0, y0, mz), Vec3::new(0.0, y0, mz - c.length * 0.32), 0.12, 0.1, wood);
            cylinder(
                &mut m,
                Vec3::new(0.0, base.y + h * 0.8, mz),
                Vec3::new(0.0, base.y + h * 0.62, mz - c.length * 0.28),
                0.1,
                0.08,
                wood,
            );
        }
        // Crow's nest on the main.
        if k == masts.len() / 2 {
            let y = base.y + h * 0.62;
            ring(&mut m, Vec3::new(0.0, y, mz), 0.9, 0.35, rail);
        }
        // Standing rigging (shrouds) to the rails.
        for side in [1.0f32, -1.0] {
            for o in [-1.2f32, 0.0, 1.2] {
                let top = base + Vec3::Y * h * 0.6;
                let foot = Vec3::new(half * plan((mz + o) / c.length + 0.5) * side * 0.95, deck_y(c, 0.5) + 0.8, mz + o);
                cylinder(&mut m, top, foot, 0.03, 0.03, [0.12, 0.1, 0.08]);
            }
        }
    }
    let bow = Vec3::new(0.0, deck_y(c, 1.0) + 0.3, c.length * 0.5);
    cylinder(&mut m, bow - Vec3::Z * 1.5, bow + Vec3::new(0.0, 2.0, c.length * 0.28), 0.22, 0.1, wood);
    if let Some(&front) = masts.last() {
        let top = Vec3::new(0.0, deck_y(c, 0.7) + c.mast_height * 0.85, front);
        cylinder(&mut m, top, bow + Vec3::new(0.0, 2.0, c.length * 0.28), 0.04, 0.04, [0.12, 0.1, 0.08]);
    }
    if style.figurehead {
        // A brass keg tap under the bowsprit.
        let brass = lin("#c9a24a");
        let at = bow + Vec3::new(0.0, -0.9, 0.4);
        cylinder(&mut m, at, at + Vec3::new(0.0, 0.0, 1.1), 0.35, 0.3, brass);
        cylinder(&mut m, at + Vec3::new(0.0, 0.0, 0.8), at + Vec3::new(0.0, -0.8, 0.9), 0.14, 0.11, brass);
        cylinder(&mut m, at + Vec3::new(0.0, 0.25, 0.4), at + Vec3::new(0.0, 0.85, 0.4), 0.1, 0.1, lin("#a8322d"));
    }
    m
}

/// Where the masts stand (z, local) for a class.
pub fn mast_positions(c: &HullClass) -> Vec<f32> {
    match c.masts {
        0 | 1 => vec![c.length * 0.08],
        2 => vec![-c.length * 0.12, c.length * 0.2],
        _ => vec![-c.length * 0.22, c.length * 0.02, c.length * 0.25],
    }
}

/// A tapered octagonal prism from `a` to `b`.
pub fn cylinder(m: &mut Mesh, a: Vec3, b: Vec3, ra: f32, rb: f32, col: [f32; 3]) {
    let axis = (b - a).normalize_or(Vec3::Y);
    let rot = Quat::from_rotation_arc(Vec3::Y, axis);
    let n = 8;
    for i in 0..n {
        let t0 = i as f32 / n as f32 * 2.0 * PI;
        let t1 = (i + 1) as f32 / n as f32 * 2.0 * PI;
        let d0 = rot * Vec3::new(t0.cos(), 0.0, t0.sin());
        let d1 = rot * Vec3::new(t1.cos(), 0.0, t1.sin());
        let (p0, p1, p2, p3) = (a + d0 * ra, a + d1 * ra, b + d1 * rb, b + d0 * rb);
        let i0 = m.vertex(p0, d0, col);
        let i1 = m.vertex(p1, d1, col);
        let i2 = m.vertex(p2, d1, col);
        let i3 = m.vertex(p3, d0, col);
        m.tri(i0, i2, i1);
        m.tri(i0, i3, i2);
    }
}

fn ring(m: &mut Mesh, center: Vec3, r: f32, h: f32, col: [f32; 3]) {
    let n = 10;
    for i in 0..n {
        let t0 = i as f32 / n as f32 * 2.0 * PI;
        let t1 = (i + 1) as f32 / n as f32 * 2.0 * PI;
        let p0 = center + Vec3::new(t0.cos() * r, 0.0, t0.sin() * r);
        let p1 = center + Vec3::new(t1.cos() * r, 0.0, t1.sin() * r);
        m.quad(p1, p0, p0 + Vec3::Y * h, p1 + Vec3::Y * h, col);
    }
}

/// Sails for every mast, in the ship's local frame: square sails between the yards or a
/// fore-and-aft main and gaff. `level` 0..2 (furled → full), `wind_rel` the wind's direction
/// relative to the bow (degrees, 0 = from ahead), `torn` 0..1 holes.
pub fn sails_mesh(c: &HullClass, style: &ShipStyle, level: f32, wind_rel: f32, time: f32, torn: f32, emblem: bool) -> Mesh {
    let mut m = Mesh::new(0);
    let sail = lin(&style.sail);
    let mark = lin(&style.emblem);
    let half = c.beam * 0.5;
    let open = (level / 2.0).clamp(0.0, 1.0);
    // Wind fills the sails forward when it comes from astern or abeam.
    let fill = (1.0 - (wind_rel.to_radians().cos() + 1.0) * 0.35).clamp(0.15, 1.0) * open;
    let flutter = (time * 3.1).sin() * 0.06 + (time * 5.3).sin() * 0.03;
    let masts = mast_positions(c);
    for (k, &mz) in masts.iter().enumerate() {
        let h = c.mast_height * if k == 0 && masts.len() > 2 { 0.9 } else { 1.0 };
        let base = deck_y(c, mz / c.length + 0.5);
        if style.rig == "square" {
            for (si, (lo, hi)) in [(0.38f32, 0.66f32), (0.68, 0.9)].iter().enumerate() {
                let y_top = base + h * hi;
                let y_bot = base + h * lo;
                let span = half * (2.2 - si as f32 * 0.45) * 0.95;
                let drop = (y_top - y_bot) * (0.08 + 0.92 * open);
                let is_main = k == masts.len() / 2 && si == 0;
                cloth(
                    &mut m,
                    14,
                    10,
                    |u, v| {
                        let x = (u - 0.5) * 2.0 * span * (1.0 - v * 0.08);
                        let y = y_top - v * drop;
                        let belly = (u * PI).sin() * (v * PI * 0.9).sin() * (1.6 * fill + flutter * open);
                        Vec3::new(x, y, mz + 0.15 + belly)
                    },
                    |u, v| {
                        if emblem && is_main && in_emblem(u, v) {
                            mark
                        } else if torn > 0.0 && hole(u, v, k as f32 * 7.0 + si as f32, torn) {
                            [0.02, 0.02, 0.02]
                        } else {
                            shade(sail, 0.92 + 0.08 * v)
                        }
                    },
                );
            }
        } else {
            // Fore-and-aft: a quadrilateral main from boom to gaff, bellied to leeward.
            let side = if turn(wind_rel) >= 0.0 { -1.0 } else { 1.0 };
            let boom = base + 1.6;
            let peak = base + h * 0.8;
            let len = c.length * 0.3;
            let rise = (peak - boom) * (0.1 + 0.9 * open);
            cloth(
                &mut m,
                10,
                10,
                |u, v| {
                    let z = mz - u * len;
                    let top = boom + rise - u * rise * 0.25;
                    let y = boom + (1.0 - v) * (top - boom);
                    let belly = (u * PI).sin() * ((1.0 - v) * PI).sin() * (1.4 * fill + flutter * open) * side;
                    Vec3::new(belly, y, z)
                },
                |u, v| {
                    if emblem && k == 0 && in_emblem(u, v) {
                        mark
                    } else if torn > 0.0 && hole(u, v, k as f32 * 5.0, torn) {
                        [0.02, 0.02, 0.02]
                    } else {
                        shade(sail, 0.9 + 0.1 * (1.0 - v))
                    }
                },
            );
            // A jib from the bowsprit.
            if k == masts.len() - 1 {
                let tip = Vec3::new(0.0, deck_y(c, 1.0) + 2.3, c.length * 0.5 + c.length * 0.25);
                let head = Vec3::new(0.0, base + h * 0.85, mz);
                let tack = Vec3::new(0.0, deck_y(c, 1.0) + 0.8, c.length * 0.45);
                let belly = Vec3::X * side * 0.8 * fill;
                let mid = (tip + head + tack) / 3.0 + belly;
                let col = sail;
                for (a, b) in [(tip, head), (head, tack), (tack, tip)] {
                    m.quad(a, b, mid, mid, col);
                    m.quad(b, a, mid, mid, col);
                }
            }
        }
    }
    m
}

fn turn(a: f32) -> f32 {
    (a + 540.0).rem_euclid(360.0) - 180.0
}

/// A brass keg tap: a round keg with a spout, centred on the sail.
fn in_emblem(u: f32, v: f32) -> bool {
    let p = Vec2::new((u - 0.5) * 1.6, v - 0.48);
    let keg = Vec2::new(p.x / 0.36, p.y / 0.26).length() < 1.0 && Vec2::new(p.x / 0.36, p.y / 0.26).length() > 0.72;
    let band = p.y.abs() < 0.03 && p.x.abs() < 0.33;
    let spout = (0.3..0.5).contains(&p.x) && (-0.02..0.06).contains(&p.y);
    let tap = (0.42..0.48).contains(&p.x) && (0.06..0.2).contains(&p.y);
    keg || band || spout || tap
}

fn hole(u: f32, v: f32, salt: f32, torn: f32) -> bool {
    let n = ((u * 12.3 + salt).sin() * (v * 9.1 + salt * 1.7).cos() * 43758.5).fract().abs();
    n < torn * 0.25
}

/// A two-sided cloth grid: `pos(u, v)` and `col(u, v)` over 0..1.
fn cloth(m: &mut Mesh, nu: usize, nv: usize, pos: impl Fn(f32, f32) -> Vec3, col: impl Fn(f32, f32) -> [f32; 3]) {
    let base = m.positions.len() as u32;
    for j in 0..=nv {
        for i in 0..=nu {
            let (u, v) = (i as f32 / nu as f32, j as f32 / nv as f32);
            m.vertex(pos(u, v), Vec3::Z, col(u, v));
        }
    }
    let w = (nu + 1) as u32;
    let first = m.indices.len();
    for j in 0..nv as u32 {
        for i in 0..nu as u32 {
            let a = base + j * w + i;
            let (b, c, d) = (a + 1, a + w + 1, a + w);
            m.tri(a, b, c);
            m.tri(a, c, d);
        }
    }
    // Normals from the triangles of this cloth only.
    let mut acc = vec![Vec3::ZERO; (nu + 1) * (nv + 1)];
    for t in m.indices[first..].as_chunks::<3>().0 {
        let [a, b, c] = [t[0], t[1], t[2]].map(|i| Vec3::from(m.positions[i as usize]));
        let n = (b - a).cross(c - a);
        for &i in t {
            acc[(i - base) as usize] += n;
        }
    }
    for (k, n) in acc.into_iter().enumerate() {
        m.normals[base as usize + k] = n.normalize_or(Vec3::Z).to_array();
    }
}

/// A pennant on the main mast top, streaming downwind (`wind_rel` from the bow, degrees).
pub fn pennant_mesh(c: &HullClass, color: [f32; 3], wind_rel: f32, time: f32) -> Mesh {
    let mut m = Mesh::new(0);
    let masts = mast_positions(c);
    let mz = masts[masts.len() / 2];
    let top = Vec3::new(0.0, deck_y(c, 0.5) + c.mast_height + 0.2, mz);
    // Streams toward where the wind goes.
    let a = (wind_rel + 180.0).to_radians();
    let dir = Vec3::new(a.sin(), 0.0, a.cos());
    cloth(
        &mut m,
        8,
        1,
        |u, v| {
            let wave = (u * 6.0 - time * 7.0).sin() * 0.25 * u;
            top + dir * u * 3.0 + Vec3::new(-dir.z, 0.0, dir.x) * wave - Vec3::Y * (v * 0.7 * (1.0 - u * 0.7))
        },
        |_, _| color,
    );
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hulls_and_sails_build_and_are_deterministic() {
        let c = HullClass { length: 32.0, beam: 8.5, masts: 2, mast_height: 22.0, guns_per_side: 8, ..Default::default() };
        let s = ShipStyle { figurehead: true, ..Default::default() };
        let a = hull_mesh(&c, &s, 1, 0.3, 7);
        let b = hull_mesh(&c, &s, 1, 0.3, 7);
        assert_eq!(a, b);
        assert!(a.indices.len() > 3000 && a.indices.len().is_multiple_of(3));
        assert!(a.positions.iter().all(|p| p.iter().all(|x| x.is_finite())));
        let sails = sails_mesh(&c, &s, 2.0, 120.0, 1.0, 0.0, true);
        assert!(!sails.is_empty());
        let furled = sails_mesh(&c, &s, 0.0, 120.0, 1.0, 0.0, true);
        let height = |m: &Mesh| {
            m.positions.iter().map(|p| p[1]).fold(f32::MIN, f32::max) - m.positions.iter().map(|p| p[1]).fold(f32::MAX, f32::min)
        };
        assert!(height(&furled) < height(&sails));
    }
}
