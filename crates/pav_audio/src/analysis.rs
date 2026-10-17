//! Looking at sound without ears: loudness, a log-frequency spectrogram (time on x, pitch on
//! y, with a tick at every C on the left edge: C1 at the bottom ... C8), and a tiny PNG
//! encoder so tools can write the image.

#![allow(clippy::needless_range_loop)]

use std::f32::consts::PI;

/// (RMS, peak) of interleaved samples. NaN or infinity anywhere gives (NaN, NaN).
pub fn stats_of(samples: &[f32]) -> (f32, f32) {
    let mut sq = 0.0f64;
    let mut peak = 0.0f32;
    for &s in samples {
        if !s.is_finite() {
            return (f32::NAN, f32::NAN);
        }
        sq += (s * s) as f64;
        peak = peak.max(s.abs());
    }
    (((sq / samples.len().max(1) as f64).sqrt()) as f32, peak)
}

/// Average spectral centroid (Hz, magnitude-weighted) of interleaved stereo: a brightness
/// measure.
pub fn spectral_centroid(samples: &[f32], sample_rate: f32) -> f32 {
    const N: usize = 4096;
    let frames = samples.len() / 2;
    let mut re = vec![0.0f32; N];
    let mut im = vec![0.0f32; N];
    let (mut num, mut den) = (0.0f64, 0.0f64);
    let mut start = 0;
    while start + N <= frames {
        for i in 0..N {
            let w = 0.5 - 0.5 * (2.0 * PI * i as f32 / N as f32).cos();
            re[i] = (samples[(start + i) * 2] + samples[(start + i) * 2 + 1]) * 0.5 * w;
            im[i] = 0.0;
        }
        fft(&mut re, &mut im);
        for b in 1..N / 2 {
            let m = ((re[b] * re[b] + im[b] * im[b]) as f64).sqrt();
            num += m * b as f64 * sample_rate as f64 / N as f64;
            den += m;
        }
        start += N;
    }
    (num / den.max(1e-12)) as f32
}

/// In-place radix-2 FFT (`re.len()` a power of two).
fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * PI / len as f32;
        let (wr, wi) = (ang.cos(), ang.sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0f32, 0.0f32);
            for k in 0..len / 2 {
                let a = start + k;
                let b = a + len / 2;
                let tr = re[b] * cr - im[b] * ci;
                let ti = re[b] * ci + im[b] * cr;
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let nr = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = nr;
            }
        }
        len <<= 1;
    }
}

fn magma(x: f32) -> [u8; 3] {
    const STOPS: [(f32, [f32; 3]); 6] = [
        (0.0, [0.0, 0.0, 4.0]),
        (0.2, [40.0, 11.0, 84.0]),
        (0.4, [101.0, 21.0, 110.0]),
        (0.6, [183.0, 55.0, 121.0]),
        (0.8, [252.0, 137.0, 97.0]),
        (1.0, [252.0, 253.0, 191.0]),
    ];
    let x = x.clamp(0.0, 1.0);
    for w in STOPS.windows(2) {
        if x <= w[1].0 {
            let k = (x - w[0].0) / (w[1].0 - w[0].0);
            let c = |i: usize| (w[0].1[i] + (w[1].1[i] - w[0].1[i]) * k) as u8;
            return [c(0), c(1), c(2)];
        }
    }
    [252, 253, 191]
}

/// Lowest and highest frequency shown.
pub const SPEC_LO: f32 = 32.7; // C1
pub const SPEC_HI: f32 = 8372.0; // C9

/// RGBA spectrogram of interleaved stereo `samples`: `width` columns over the whole length,
/// `height` rows from [`SPEC_LO`] (bottom) to [`SPEC_HI`] (top) on a log scale, 70 dB of
/// range below the loudest point.
pub fn spectrogram(samples: &[f32], sample_rate: f32, width: usize, height: usize) -> Vec<u8> {
    let (w, h) = (width.max(1), height.max(1));
    let frames = samples.len() / 2;
    const N: usize = 4096;
    let window: Vec<f32> = (0..N).map(|i| 0.5 - 0.5 * (2.0 * PI * i as f32 / N as f32).cos()).collect();
    let bin_hz = sample_rate / N as f32;
    let hi = SPEC_HI.min(sample_rate * 0.5);
    let row_f = |y: f32| SPEC_LO * (hi / SPEC_LO).powf(y / h as f32);
    let mut db = vec![-200.0f32; w * h];
    let mut re = vec![0.0f32; N];
    let mut im = vec![0.0f32; N];
    let mut mag = vec![0.0f32; N / 2];
    for x in 0..w {
        let c = (x as f64 + 0.5) / w as f64 * frames as f64;
        let s0 = c as i64 - N as i64 / 2;
        for i in 0..N {
            let k = s0 + i as i64;
            let v =
                if k >= 0 && (k as usize) < frames { (samples[k as usize * 2] + samples[k as usize * 2 + 1]) * 0.5 } else { 0.0 };
            re[i] = v * window[i];
            im[i] = 0.0;
        }
        fft(&mut re, &mut im);
        for b in 0..N / 2 {
            mag[b] = re[b] * re[b] + im[b] * im[b];
        }
        for y in 0..h {
            // Row y counts from the bottom.
            let f0 = row_f(y as f32);
            let f1 = row_f(y as f32 + 1.0);
            let b0 = f0 / bin_hz;
            let b1 = f1 / bin_hz;
            let p = if b1 - b0 < 1.0 {
                let b = (b0 + b1) * 0.5;
                let i = (b as usize).min(N / 2 - 2);
                let k = b - i as f32;
                mag[i] * (1.0 - k) + mag[i + 1] * k
            } else {
                let mut m = 0.0f32;
                for b in (b0 as usize)..=(b1 as usize).min(N / 2 - 1) {
                    m = m.max(mag[b]);
                }
                m
            };
            db[(h - 1 - y) * w + x] = 10.0 * p.max(1e-20).log10();
        }
    }
    let top = db.iter().copied().fold(-200.0f32, f32::max);
    let mut out = vec![0u8; w * h * 4];
    for i in 0..w * h {
        let v = ((db[i] - top + 70.0) / 70.0).clamp(0.0, 1.0);
        let [r, g, b] = magma(v);
        out[i * 4] = r;
        out[i * 4 + 1] = g;
        out[i * 4 + 2] = b;
        out[i * 4 + 3] = 255;
    }
    // Ticks at each C (C1..C8) on the left edge.
    let mut f = SPEC_LO;
    while f < hi {
        let y = ((f / SPEC_LO).ln() / (hi / SPEC_LO).ln() * h as f32) as usize;
        if y < h {
            let row = h - 1 - y;
            for x in 0..w.min(10) {
                let i = (row * w + x) * 4;
                out[i] = 120;
                out[i + 1] = 220;
                out[i + 2] = 255;
            }
        }
        f *= 2.0;
    }
    out
}

fn crc32(data: &[u8], crc: u32) -> u32 {
    let mut c = !crc;
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    !c
}

/// Encodes RGBA pixels as a PNG (uncompressed deflate: big, simple, dependency-free).
pub fn png_rgba(width: usize, height: usize, rgba: &[u8]) -> Vec<u8> {
    let mut raw = Vec::with_capacity((width * 4 + 1) * height);
    for y in 0..height {
        raw.push(0);
        raw.extend_from_slice(&rgba[y * width * 4..(y + 1) * width * 4]);
    }
    let mut z = vec![0x78, 0x01];
    let chunks: Vec<&[u8]> = raw.chunks(65535).collect();
    for (i, c) in chunks.iter().enumerate() {
        z.push(if i + 1 == chunks.len() { 1 } else { 0 });
        let len = c.len() as u16;
        z.extend_from_slice(&len.to_le_bytes());
        z.extend_from_slice(&(!len).to_le_bytes());
        z.extend_from_slice(c);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &x in &raw {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut chunk = |kind: &[u8; 4], data: &[u8]| {
        png.extend_from_slice(&(data.len() as u32).to_be_bytes());
        png.extend_from_slice(kind);
        png.extend_from_slice(data);
        let crc = crc32(data, crc32(kind, 0));
        png.extend_from_slice(&crc.to_be_bytes());
    };
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(width as u32).to_be_bytes());
    ihdr.extend_from_slice(&(height as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(b"IHDR", &ihdr);
    chunk(b"IDAT", &z);
    chunk(b"IEND", &[]);
    png
}
