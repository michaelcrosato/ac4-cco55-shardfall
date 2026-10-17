//! Headless UI capture: runs an egui frame and rasterises its tessellated output on the CPU
//! (triangles with UVs into the font atlas and vertex colours, premultiplied alpha blending,
//! clip rects), so tools and tests see exactly what the window shows, without a GPU.
//!
//! ```no_run
//! let theme = pav_ui::Theme::default();
//! let rgba = pav_ui::raster::render([1920, 1080], 1.0, |ctx| pav_ui::theme::install(ctx, &theme), |ctx| {
//!     pav_ui::hud::draw_sample(ctx, &theme);
//! });
//! std::fs::write("hud.png", pav_ui::raster::encode_png(1920, 1080, &rgba)).unwrap();
//! ```

use std::collections::BTreeMap;

use egui::epaint::{ClippedPrimitive, ImageData, Primitive, TextureId, Vertex};
use egui::{Pos2, Rect};

/// A texture in the rasteriser: premultiplied sRGBA8.
struct Texture {
    size: [usize; 2],
    pixels: Vec<[u8; 4]>,
}

/// Runs `draw` in an egui context of `size` physical pixels at `pixels_per_point` and returns
/// the frame as RGBA8 with straight alpha over transparent black. `theme_setup` runs once on
/// the new context first (e.g. [`crate::theme::install`]). The frame runs three times (fonts and
/// the atlas warm up, layout settles) and the last one is rasterised.
pub fn render(
    size: [u32; 2],
    pixels_per_point: f32,
    theme_setup: impl Fn(&egui::Context),
    mut draw: impl FnMut(&egui::Context),
) -> Vec<u8> {
    let ctx = egui::Context::default();
    theme_setup(&ctx);
    let ppp = pixels_per_point.max(0.05);
    let screen = Rect::from_min_size(Pos2::ZERO, egui::vec2(size[0] as f32 / ppp, size[1] as f32 / ppp));
    let mut textures: BTreeMap<TextureId, Texture> = BTreeMap::new();
    let mut last = None;
    for pass in 0..3 {
        let mut raw = egui::RawInput {
            screen_rect: Some(screen),
            max_texture_side: Some(8192),
            time: Some(pass as f64 / 60.0),
            predicted_dt: 1.0 / 60.0,
            ..Default::default()
        };
        raw.viewports.entry(egui::ViewportId::ROOT).or_default().native_pixels_per_point = Some(ppp);
        let mut out = ctx.run_ui(raw, |ui| draw(ui.ctx()));
        apply_textures(&mut textures, &out.textures_delta);
        out.textures_delta.clear();
        last = Some((out.shapes, out.pixels_per_point));
    }
    let (shapes, ppp_out) = last.expect("ran");
    let prims = ctx.tessellate(shapes, ppp_out);
    let mut canvas = Canvas::new(size[0] as usize, size[1] as usize);
    for p in &prims {
        canvas.draw(p, ppp_out, &textures);
    }
    canvas.into_straight()
}

fn apply_textures(textures: &mut BTreeMap<TextureId, Texture>, delta: &egui::TexturesDelta) {
    for (id, deltas) in &delta.set {
        for d in deltas {
            apply_delta(textures, *id, d);
        }
    }
    for id in &delta.free {
        textures.remove(id);
    }
}

fn apply_delta(textures: &mut BTreeMap<TextureId, Texture>, id: TextureId, d: &egui::epaint::ImageDelta) {
    let id = &id;
    {
        let ImageData::Color(img) = &d.image;
        let pixels: Vec<[u8; 4]> = img.pixels.iter().map(|c| c.to_array()).collect();
        match d.pos {
            None => {
                textures.insert(*id, Texture { size: img.size, pixels });
            }
            Some([x0, y0]) => {
                if let Some(t) = textures.get_mut(id) {
                    for y in 0..img.size[1] {
                        for x in 0..img.size[0] {
                            let (tx, ty) = (x0 + x, y0 + y);
                            if tx < t.size[0] && ty < t.size[1] {
                                t.pixels[ty * t.size[0] + tx] = pixels[y * img.size[0] + x];
                            }
                        }
                    }
                }
            }
        }
    }
}

/// A premultiplied RGBA8 framebuffer.
struct Canvas {
    w: usize,
    h: usize,
    px: Vec<[u8; 4]>,
}

#[inline]
fn blend(dst: &mut [u8; 4], src: [f32; 4]) {
    // Premultiplied "over": dst = src + dst × (1 - src.a), all in 0..255.
    let inv = 1.0 - src[3] / 255.0;
    for i in 0..4 {
        let v = src[i] + dst[i] as f32 * inv;
        dst[i] = v.clamp(0.0, 255.0).round() as u8;
    }
}

#[inline]
fn blend_u8(dst: &mut [u8; 4], src: [u8; 4]) {
    let inv = 255 - src[3] as u32;
    for i in 0..4 {
        let v = src[i] as u32 + (dst[i] as u32 * inv + 127) / 255;
        dst[i] = v.min(255) as u8;
    }
}

impl Canvas {
    fn new(w: usize, h: usize) -> Self {
        Self { w, h, px: vec![[0; 4]; w * h] }
    }

    fn draw(&mut self, prim: &ClippedPrimitive, ppp: f32, textures: &BTreeMap<TextureId, Texture>) {
        let Primitive::Mesh(mesh) = &prim.primitive else { return };
        let Some(tex) = textures.get(&mesh.texture_id) else { return };
        let c = prim.clip_rect;
        let clip = [
            ((c.min.x * ppp).round().max(0.0) as i64).min(self.w as i64),
            ((c.min.y * ppp).round().max(0.0) as i64).min(self.h as i64),
            ((c.max.x * ppp).round().max(0.0) as i64).min(self.w as i64),
            ((c.max.y * ppp).round().max(0.0) as i64).min(self.h as i64),
        ];
        if clip[0] >= clip[2] || clip[1] >= clip[3] {
            return;
        }
        for t in mesh.indices.as_chunks::<3>().0 {
            let v = [t[0], t[1], t[2]].map(|i| mesh.vertices.get(i as usize).copied());
            if let [Some(a), Some(b), Some(c)] = v {
                self.triangle([a, b, c], ppp, clip, tex);
            }
        }
    }

    fn triangle(&mut self, v: [Vertex; 3], ppp: f32, clip: [i64; 4], tex: &Texture) {
        let p = v.map(|x| [x.pos.x * ppp, x.pos.y * ppp]);
        let area = (p[1][0] - p[0][0]) * (p[2][1] - p[0][1]) - (p[1][1] - p[0][1]) * (p[2][0] - p[0][0]);
        if !area.is_finite() || area.abs() < 1e-8 {
            return;
        }
        if v.iter().all(|x| x.color.a() == 0 && x.color.r() == 0 && x.color.g() == 0 && x.color.b() == 0) {
            return;
        }
        let ymin = p[0][1].min(p[1][1]).min(p[2][1]);
        let ymax = p[0][1].max(p[1][1]).max(p[2][1]);
        // Rows whose pixel centre is in [ymin, ymax): top edges in, bottom edges out.
        let row0 = ((ymin - 0.5).ceil() as i64).max(clip[1]);
        let row1 = ((ymax - 0.5).ceil() as i64 - 1).min(clip[3] - 1);
        if row0 > row1 {
            return;
        }
        let same_uv = v[0].uv == v[1].uv && v[1].uv == v[2].uv;
        let same_col = v[0].color == v[1].color && v[1].color == v[2].color;
        let flat: Option<[u8; 4]> = if same_uv && same_col {
            let t = sample(tex, v[0].uv);
            let c = v[0].color.to_array();
            Some(std::array::from_fn(|i| ((t[i] * c[i] as f32) / 255.0).round().clamp(0.0, 255.0) as u8))
        } else {
            None
        };
        if flat.is_some_and(|f| f[3] == 0 && f[0] == 0 && f[1] == 0 && f[2] == 0) {
            return;
        }
        let tex_flat = if same_uv { Some(sample(tex, v[0].uv)) } else { None };
        let inv_area = 1.0 / area;
        let cols = v.map(|x| x.color.to_array().map(|c| c as f32));
        // Inside means: on the same side of every edge as the triangle's winding says (one
        // sign for the whole triangle, so slivers can't flip). Each edge's crossing is computed
        // from its endpoints in a canonical (top to bottom) order, so a shared edge gives both
        // triangles exactly the same x and no pixel is drawn twice.
        let s = area.signum();
        let edges = [(0usize, 1usize), (1, 2), (2, 0)];
        for row in row0..=row1 {
            let yc = row as f32 + 0.5;
            let mut xl = f32::NEG_INFINITY;
            let mut xr = f32::INFINITY;
            for &(a, b) in &edges {
                let dy = p[b][1] - p[a][1];
                if dy == 0.0 {
                    continue; // horizontal: the row range handles it
                }
                let (pa, pb) = if dy > 0.0 { (p[a], p[b]) } else { (p[b], p[a]) };
                let x = pa[0] + (yc - pa[1]) * (pb[0] - pa[0]) / (pb[1] - pa[1]);
                if dy * s < 0.0 {
                    xl = xl.max(x); // left bound, inclusive
                } else {
                    xr = xr.min(x); // right bound, exclusive
                }
            }
            if xl >= xr {
                continue;
            }
            let x0 = ((xl - 0.5).ceil() as i64).max(clip[0]);
            let x1 = ((xr - 0.5).ceil() as i64 - 1).min(clip[2] - 1);
            if x0 > x1 {
                continue;
            }
            let base = row as usize * self.w;
            if let Some(f) = flat {
                for x in x0..=x1 {
                    blend_u8(&mut self.px[base + x as usize], f);
                }
                continue;
            }
            for x in x0..=x1 {
                let xc = x as f32 + 0.5;
                // Barycentric weights.
                let w0 = ((p[1][0] - xc) * (p[2][1] - yc) - (p[1][1] - yc) * (p[2][0] - xc)) * inv_area;
                let w1 = ((p[2][0] - xc) * (p[0][1] - yc) - (p[2][1] - yc) * (p[0][0] - xc)) * inv_area;
                let w2 = 1.0 - w0 - w1;
                let col: [f32; 4] =
                    if same_col { cols[0] } else { std::array::from_fn(|i| cols[0][i] * w0 + cols[1][i] * w1 + cols[2][i] * w2) };
                let t = match tex_flat {
                    Some(t) => t,
                    None => {
                        let u = v[0].uv.x * w0 + v[1].uv.x * w1 + v[2].uv.x * w2;
                        let vv = v[0].uv.y * w0 + v[1].uv.y * w1 + v[2].uv.y * w2;
                        sample(tex, Pos2::new(u, vv))
                    }
                };
                let src: [f32; 4] = std::array::from_fn(|i| t[i] * col[i] / 255.0);
                if src[3] <= 0.0 && src[0] <= 0.0 && src[1] <= 0.0 && src[2] <= 0.0 {
                    continue;
                }
                blend(&mut self.px[base + x as usize], src);
            }
        }
    }

    /// Un-premultiplies into straight-alpha RGBA8 bytes.
    fn into_straight(self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.w * self.h * 4);
        for p in self.px {
            let a = p[3];
            if a == 0 {
                out.extend_from_slice(&[0, 0, 0, 0]);
            } else if a == 255 {
                out.extend_from_slice(&p);
            } else {
                let f = |c: u8| ((c as u32 * 255 + a as u32 / 2) / a as u32).min(255) as u8;
                out.extend_from_slice(&[f(p[0]), f(p[1]), f(p[2]), a]);
            }
        }
        out
    }
}

/// Bilinear sample with clamping, premultiplied, as f32 0..255.
#[inline]
fn sample(tex: &Texture, uv: Pos2) -> [f32; 4] {
    let (w, h) = (tex.size[0], tex.size[1]);
    if w == 0 || h == 0 {
        return [255.0; 4];
    }
    let x = (uv.x * w as f32 - 0.5).clamp(0.0, (w - 1) as f32);
    let y = (uv.y * h as f32 - 0.5).clamp(0.0, (h - 1) as f32);
    let (x0, y0) = (x.floor() as usize, y.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let p = |xx: usize, yy: usize| tex.pixels[yy * w + xx];
    let (a, b, c, d) = (p(x0, y0), p(x1, y0), p(x0, y1), p(x1, y1));
    std::array::from_fn(|i| {
        let top = a[i] as f32 + (b[i] as f32 - a[i] as f32) * fx;
        let bot = c[i] as f32 + (d[i] as f32 - c[i] as f32) * fx;
        top + (bot - top) * fy
    })
}

/// Draws straight-alpha RGBA8 `overlay` over `base` (same size; both straight alpha).
pub fn composite_over(base_rgba: &mut [u8], overlay: &[u8]) {
    for (b, o) in base_rgba.as_chunks_mut::<4>().0.iter_mut().zip(overlay.as_chunks::<4>().0) {
        let oa = o[3] as f32 / 255.0;
        if oa <= 0.0 {
            continue;
        }
        if oa >= 1.0 {
            b.copy_from_slice(o);
            continue;
        }
        let ba = b[3] as f32 / 255.0;
        let out_a = oa + ba * (1.0 - oa);
        for i in 0..3 {
            let c = (o[i] as f32 * oa + b[i] as f32 * ba * (1.0 - oa)) / out_a.max(1e-6);
            b[i] = c.round().clamp(0.0, 255.0) as u8;
        }
        b[3] = (out_a * 255.0).round() as u8;
    }
}

/// Encodes RGBA8 as a PNG (stored, uncompressed deflate blocks: no dependencies).
pub fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let (w, h) = (width as usize, height as usize);
    assert_eq!(rgba.len(), w * h * 4, "encode_png: buffer size");
    // Raw scanlines with filter byte 0.
    let mut raw = Vec::with_capacity((w * 4 + 1) * h);
    for row in rgba.chunks_exact(w * 4) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    // zlib stream: header, stored blocks, adler32.
    let mut z = Vec::with_capacity(raw.len() + raw.len() / 65535 * 5 + 16);
    z.extend_from_slice(&[0x78, 0x01]);
    let mut chunks = raw.chunks(65535).peekable();
    if chunks.peek().is_none() {
        z.extend_from_slice(&[1, 0, 0, 0xff, 0xff]);
    }
    while let Some(c) = chunks.next() {
        let last = chunks.peek().is_none();
        z.push(last as u8);
        let len = c.len() as u16;
        z.extend_from_slice(&len.to_le_bytes());
        z.extend_from_slice(&(!len).to_le_bytes());
        z.extend_from_slice(c);
    }
    z.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut png = Vec::with_capacity(z.len() + 64);
    png.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit RGBA, deflate, adaptive filters, no interlace
    chunk(&mut png, b"IHDR", &ihdr);
    chunk(&mut png, b"IDAT", &z);
    chunk(&mut png, b"IEND", &[]);
    png
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

fn crc32(data: &[u8]) -> u32 {
    static TABLE: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();
    let table = TABLE.get_or_init(|| {
        std::array::from_fn(|n| {
            let mut c = n as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
            }
            c
        })
    });
    let mut c = 0xffff_ffffu32;
    for &b in data {
        c = table[((c ^ b as u32) & 0xff) as usize] ^ (c >> 8);
    }
    c ^ 0xffff_ffff
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for chunk in data.chunks(5552) {
        for &x in chunk {
            a += x as u32;
            b += a;
        }
        a %= 65521;
        b %= 65521;
    }
    (b << 16) | a
}

/// The share of pixels with any coverage (alpha > 0), 0..1: a quick "drew something" check.
pub fn coverage(rgba: &[u8]) -> f32 {
    let n = rgba.len() / 4;
    if n == 0 {
        return 0.0;
    }
    rgba.as_chunks::<4>().0.iter().filter(|p| p[3] > 0).count() as f32 / n as f32
}
