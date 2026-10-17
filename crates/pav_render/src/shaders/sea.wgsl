// The sea and the sky (compiled after common.wgsl): the ocean surface, a camera-centred LOD grid
// displaced by the same six Gerstner waves as `pav_core::ocean` (so ships and swimmers bob with
// what is drawn); the sky behind everything, with the sun and procedural clouds; rain streaks
// around the camera. Everything is procedural: no textures besides the seabed's heights.

struct Sea {
    center: vec4<f32>,          // xy grid centre (world x, z), z sea level, w sea time (s)
    wave_a: array<vec4<f32>, 6>,// dir x, dir z, k (rad/m), amplitude (m)
    wave_b: array<vec4<f32>, 6>,// Gerstner Q, phase at the grid centre now (rad), wavelength (m), omega
    deep: vec4<f32>,            // rgb open water; w ripples (0..1)
    mid: vec4<f32>,             // rgb mid water; w crest foam (0..1)
    shallow: vec4<f32>,         // rgb shallows; w sun glint
    foam: vec4<f32>,            // rgb foam; w shore foam depth (m)
    refl_zenith: vec4<f32>,     // rgb reflected sky overhead; w rain on the water (0..1)
    refl_horizon: vec4<f32>,    // rgb reflected sky at the horizon; w 1 = ocean on
    seabed: vec4<f32>,          // xy origin (world x, z); zw uv per metre
    seabed2: vec4<f32>,         // x floor height outside the grid; y 1 = seabed on; zw half a cell in uv
    fade: vec4<f32>,            // a wave's geometry fades out between x·λ and y·λ from the eye; all of it from z to w m
    misc: vec4<f32>,            // xy unit wind direction; z sum of Q·k·A; w sum of amplitudes
    zenith: vec4<f32>,          // rgb sky overhead; w 1 = sky on
    horizon: vec4<f32>,         // rgb sky at the horizon; w storm (0..1)
    ground: vec4<f32>,          // rgb below the horizon; w lightning flash (0..1)
    to_sun: vec4<f32>,          // xyz unit vector toward the sun; w disc radius (rad)
    sun_disc: vec4<f32>,        // rgb disc (HDR); w glow
    cloud: vec4<f32>,           // x cover, y density, z height (m), w curvature
    cloud2: vec4<f32>,          // xy wind offset (m), z 1 / cloud size (1/m), w time (s)
    cloud_lit: vec4<f32>,       // rgb sunlit tops
    cloud_shade: vec4<f32>,     // rgb shaded undersides
    rain: vec4<f32>,            // xy wind (m/s); z amount (0..1); w fall speed (m/s)
    rain_color: vec4<f32>,      // rgb streaks
};

@group(1) @binding(0) var<uniform> sea: Sea;
@group(1) @binding(1) var seabed_tex: texture_2d<f32>;
@group(1) @binding(2) var sea_samp: sampler;

const TAU: f32 = 6.2831853;

struct SeaFsOut {
    @location(0) color: vec4<f32>,
    @location(1) normal: vec4<f32>,
};

// ---------------------------------------------------------------- noise

fn sea_hash(p: vec2<i32>) -> f32 {
    var h = (bitcast<u32>(p.x) * 0x8da6b343u) ^ (bitcast<u32>(p.y) * 0xd8163841u);
    h = h * 747796405u + 2891336453u;
    h = ((h >> ((h >> 28u) + 4u)) ^ h) * 277803737u;
    h = (h >> 22u) ^ h;
    return f32(h & 0xffffffu) * (1.0 / 16777216.0);
}

fn hash_u(n: u32) -> f32 {
    var h = n * 747796405u + 2891336453u;
    h = ((h >> ((h >> 28u) + 4u)) ^ h) * 277803737u;
    h = (h >> 22u) ^ h;
    return f32(h & 0xffffffu) * (1.0 / 16777216.0);
}

// Value noise (0..1).
fn vn(p: vec2<f32>) -> f32 {
    let fl = floor(p);
    let i = vec2<i32>(fl);
    let f = p - fl;
    let u = f * f * (3.0 - 2.0 * f);
    let a = sea_hash(i);
    let b = sea_hash(i + vec2<i32>(1, 0));
    let c = sea_hash(i + vec2<i32>(0, 1));
    let d = sea_hash(i + vec2<i32>(1, 1));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

// Value noise and its gradient: (value, d/dx, d/dy).
fn vnd(p: vec2<f32>) -> vec3<f32> {
    let fl = floor(p);
    let i = vec2<i32>(fl);
    let f = p - fl;
    let u = f * f * (3.0 - 2.0 * f);
    let du = 6.0 * f * (1.0 - f);
    let a = sea_hash(i);
    let b = sea_hash(i + vec2<i32>(1, 0));
    let c = sea_hash(i + vec2<i32>(0, 1));
    let d = sea_hash(i + vec2<i32>(1, 1));
    let k = a - b - c + d;
    let v = a + (b - a) * u.x + (c - a) * u.y + k * u.x * u.y;
    return vec3<f32>(v, du * (vec2<f32>(b - a, c - a) + k * u.yx));
}

// Fractal noise for clouds (0..1, mean about 0.5).
fn fbm(p0: vec2<f32>) -> f32 {
    var p = p0;
    var s = 0.0;
    var a = 0.5;
    for (var i = 0; i < 5; i++) {
        s += a * vn(p);
        p = vec2<f32>(p.x * 1.6 - p.y * 1.2, p.x * 1.2 + p.y * 1.6) + vec2<f32>(5.3, 1.7);
        a *= 0.5;
    }
    return s / 0.96875;
}

// ---------------------------------------------------------------- sky

// The sky's colour along `rd` without sun disc or clouds: the zenith-to-horizon gradient, a
// warm glow around the sun near the horizon, the ground colour below. `zen`/`hor` are the
// gradient's ends (the sky's own, or the colours the ocean reflects).
fn sky_gradient(rd: vec3<f32>, zen: vec3<f32>, hor: vec3<f32>) -> vec3<f32> {
    let up = clamp(rd.y, 0.0, 1.0);
    // A painterly curve: a broad pale band at the horizon, deepening quickly above it.
    let t = 1.0 - pow(1.0 - up, 3.5);
    var col = mix(hor, zen, t);
    if (sea.zenith.w > 0.5) {
        let cs = dot(rd, sea.to_sun.xyz);
        let glow = sea.sun_disc.w;
        let tint = sea.sun_disc.rgb / max(max(sea.sun_disc.r, max(sea.sun_disc.g, sea.sun_disc.b)), 1e-3);
        // Around the sun, strongest low in the sky (the evening glow).
        let near = pow(max(cs * 0.5 + 0.5, 0.0), 6.0) * glow * (1.0 - t * 0.7);
        col = mix(col, hor * 0.6 + tint * max(hor.r, max(hor.g, hor.b)) * 0.9, clamp(near, 0.0, 1.0) * 0.75);
        col += sea.sun_disc.rgb * (pow(max(cs, 0.0), 24.0) * 0.012 * glow);
    }
    if (rd.y < 0.0) {
        col = mix(hor, sea.ground.rgb, smoothstep(0.0, 0.2, -rd.y));
    }
    return col;
}

// Fog toward the horizon for a ray that never ends: in ray mode, the air's whole thickness
// along `rd` (it thins with height, so looking up there is little); otherwise a band at the
// horizon.
fn sky_fog_amount(rd: vec3<f32>) -> f32 {
    if (g.fog.w < 0.5) {
        return 0.0;
    }
    if (g.fog3.x > 0.5 && g.fog3.y > 0.0) {
        let k = g.fog3.y;
        let h0 = max(g.eye.y - g.fog3.z, 0.0);
        let thick = exp(-k * h0) / (k * max(rd.y, 1e-5));
        return smoothstep(g.fog2.z, g.fog2.w, thick);
    }
    return 1.0 - smoothstep(0.0, 0.12, rd.y);
}

// Clouds along `rd`: rgb colour, a coverage.
fn clouds(rd: vec3<f32>) -> vec4<f32> {
    let cover = sea.cloud.x;
    if (cover <= 0.0 || rd.y < -0.01) {
        return vec4<f32>(0.0);
    }
    // A plane over the camera that bends down toward the horizon: far clouds stay visible and
    // crowd together near the horizon.
    let q = rd.xz / (max(rd.y, 0.0) + sea.cloud.w) * sea.cloud.z + sea.cloud2.xy;
    let uv = q * sea.cloud2.z;
    let tt = sea.cloud2.w * 0.01;
    let warp = vec2<f32>(vn(uv * 0.5 + vec2<f32>(tt, 0.0)), vn(uv * 0.5 + vec2<f32>(3.1, tt))) - 0.5;
    let p = uv + warp * 0.8;
    let n = fbm(p);
    let storm = sea.horizon.w;
    let th = mix(0.7, 0.26, clamp(cover, 0.0, 1.0));
    let cov = smoothstep(th, th + 0.11 + 0.08 * storm, n) * sea.cloud.y;
    if (cov <= 0.001) {
        return vec4<f32>(0.0);
    }
    // Light from the sun: the side facing it is thinner (brighter), the cores are thick.
    let sd = normalize(sea.to_sun.xz + vec2<f32>(1e-4, 0.0));
    let ns = fbm(p + sd * 0.12);
    let lit = clamp(0.6 + (n - ns) * 5.0, 0.0, 1.0) * clamp(sea.to_sun.y * 4.0 + 0.6, 0.25, 1.0);
    let thick = smoothstep(th + 0.05, th + 0.4, n);
    // Painterly: soft steps between shade and light.
    let band = smoothstep(0.25, 0.45, lit) * 0.6 + smoothstep(0.65, 0.85, lit) * 0.4;
    var c = mix(sea.cloud_shade.rgb, sea.cloud_lit.rgb, band * (1.0 - thick * 0.45));
    // Silver lining when looking toward the sun through thin edges.
    let cs = max(dot(rd, sea.to_sun.xyz), 0.0);
    c += sea.sun_disc.rgb * (pow(cs, 10.0) * (1.0 - thick) * 0.06);
    c *= 1.0 - 0.5 * storm * thick;
    // Fade into the haze at the horizon.
    let fade = smoothstep(0.01, 0.09, rd.y);
    return vec4<f32>(c, cov * fade);
}

@vertex
fn vs_sky(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((vi << 1u) & 2u), f32(vi & 2u));
    // Depth 0 is the far plane: the pipeline passes only pixels nothing was drawn on.
    return vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fs_sky(@builtin(position) frag: vec4<f32>) -> SeaFsOut {
    let ndc = vec2<f32>(frag.x * g.viewport.z * 2.0 - 1.0, 1.0 - frag.y * g.viewport.w * 2.0);
    var rd = g.forward.xyz;
    if (g.eye.w > 0.5) {
        let w = g.inv_view_proj * vec4<f32>(ndc, 0.5, 1.0);
        rd = normalize(w.xyz / w.w - g.eye.xyz);
    }
    var col = sky_gradient(rd, sea.zenith.rgb, sea.horizon.rgb);
    let cl = clouds(rd);
    col = mix(col, cl.rgb, cl.a);
    // Lightning lights the clouds, brightest around the bolt (a new place every few seconds).
    let flash = sea.ground.w;
    if (flash > 0.0) {
        let az = hash_u(u32(max(sea.cloud2.w, 0.0) * 0.4)) * TAU;
        let bolt = normalize(vec3<f32>(cos(az), 0.3, sin(az)));
        let spot = pow(max(dot(rd, bolt), 0.0), 5.0);
        col += vec3<f32>(0.8, 0.86, 1.0) * (flash * (0.06 + cl.a * (0.2 + 1.6 * spot)));
    }
    let fogged = sky_fog_amount(rd);
    col = mix(col, fog_tint(rd), fogged);
    // The sun's disc and halo, through the clouds and (dimmed) through the haze.
    let cs = dot(rd, sea.to_sun.xyz);
    let ang = acos(clamp(cs, -1.0, 1.0));
    let r = sea.to_sun.w;
    let through = (1.0 - 0.8 * cl.a) * (1.0 - 0.3 * fogged);
    let disc = (1.0 - smoothstep(r * 0.8, r * 1.2, ang)) * smoothstep(-0.004, 0.004, rd.y);
    let halo = exp(-ang / max(r * 2.5, 1e-4)) * 0.25 + exp(-ang / max(r * 12.0, 1e-4)) * 0.04;
    col += sea.sun_disc.rgb * (halo * sea.sun_disc.w * mix(0.35, 1.0, through)) * step(-0.02, rd.y);
    col = mix(col, sea.sun_disc.rgb * (1.0 - 0.35 * fogged), disc * through);
    var o: SeaFsOut;
    o.color = vec4<f32>(col, 1.0);
    o.normal = vec4<f32>(0.0);
    return o;
}

// ---------------------------------------------------------------- ocean

struct OceanOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    // The rest position relative to the grid centre: the waves are evaluated there.
    @location(1) rest: vec2<f32>,
};

fn wrap_phase(ph: f32) -> f32 {
    return ph - TAU * floor(ph / TAU);
}

// How much of wave `i` the grid shows at `dist` metres from the eye (it fades where the grid
// is too coarse for it, and everything fades before the grid's last ring meets the skirt).
fn wave_fade(i: u32, dist: f32) -> f32 {
    let len = sea.wave_b[i].z;
    return (1.0 - smoothstep(sea.fade.x * len, sea.fade.y * len, dist)) * (1.0 - smoothstep(sea.fade.z, sea.fade.w, dist));
}

// `pav_core::ocean::SeaState::displacement`, with the phase measured from the grid centre:
// where the water resting at centre + r is now (dx, height, dz).
fn gerstner(r: vec2<f32>, dist: f32) -> vec3<f32> {
    var d = vec3<f32>(0.0);
    for (var i = 0u; i < 6u; i++) {
        let a = sea.wave_a[i];
        let b = sea.wave_b[i];
        let amp = a.w * wave_fade(i, dist);
        let ph = wrap_phase(a.z * dot(a.xy, r) + b.y);
        let s = sin(ph);
        let c = cos(ph);
        d.x += b.x * amp * a.x * c;
        d.z += b.x * amp * a.y * c;
        d.y += amp * s;
    }
    return d;
}

// One grid vertex: xy its rest position relative to the grid centre (m), z the spacing of its
// ring (0 on the skirt), w the ring's half size. Near a ring's outer edge the vertices slide
// onto the next ring's coarser lattice, so neighbouring rings meet without cracks or pops.
@vertex
fn vs_ocean(@location(0) v: vec4<f32>) -> OceanOut {
    var r = v.xy;
    let s = v.z;
    if (s > 0.0) {
        let h = v.w;
        let edge = max(abs(r.x), abs(r.y));
        let m = clamp((edge - 0.72 * h) / (0.2 * h), 0.0, 1.0);
        let gi = r / s;
        let odd = gi - 2.0 * floor(gi * 0.5);
        r = r - odd * s * m;
    }
    let p0 = sea.center.xy + r;
    let dist = length(p0 - g.eye.xz);
    let d = gerstner(r, dist);
    let world = vec3<f32>(p0.x + d.x, sea.center.z + d.y, p0.y + d.z);
    var o: OceanOut;
    var clip = g.view_proj * vec4<f32>(world, 1.0);
    // Beyond the far plane (reversed depth < 0) the sea is kept just inside it, so it runs on
    // to the horizon instead of ending in a cut.
    if (clip.w > 0.0) {
        clip.z = max(clip.z, clip.w * 1e-7);
    }
    o.clip = clip;
    o.world = world;
    o.rest = r;
    return o;
}

// The sky the water mirrors along `rd`: the ocean's reflection colours, lightened by cloud.
fn reflected_sky(rd0: vec3<f32>) -> vec3<f32> {
    let rd = vec3<f32>(rd0.x, abs(rd0.y), rd0.z);
    var col = sky_gradient(rd, sea.refl_zenith.rgb, sea.refl_horizon.rgb);
    if (sea.zenith.w > 0.5 && sea.cloud.x > 0.0) {
        let avg = mix(sea.cloud_shade.rgb, sea.cloud_lit.rgb, 0.6);
        col = mix(col, avg, clamp(sea.cloud.x * sea.cloud.y, 0.0, 1.0) * 0.45 * smoothstep(0.02, 0.4, rd.y));
    }
    return col;
}

// Seabed height under the point `xz` (m): the seabed's heights (bilinear, as `Seabed::height`),
// the open-sea floor outside them; very deep without a seabed.
fn seabed_at(xz: vec2<f32>) -> f32 {
    if (sea.seabed2.y < 0.5) {
        return -1e4;
    }
    let uv = (xz - sea.seabed.xy) * sea.seabed.zw;
    let half = sea.seabed2.zw;
    var bed = sea.seabed2.x;
    if (uv.x >= half.x && uv.y >= half.y && uv.x <= 1.0 - half.x && uv.y <= 1.0 - half.y) {
        bed = textureSampleLevel(seabed_tex, sea_samp, uv, 0.0).r;
    }
    return bed;
}

// The sun's shadow on the water: one filtered tap is enough on a moving surface.
fn water_shadow(p: vec3<f32>) -> f32 {
    if (g.sun_dir.w < 0.5) {
        return 1.0;
    }
    let lp = g.light_view_proj * vec4<f32>(p, 1.0);
    let ndc = lp.xyz / lp.w;
    let uv = vec2<f32>(ndc.x * 0.5 + 0.5, -ndc.y * 0.5 + 0.5);
    if (uv.x < 0.0 || uv.y < 0.0 || uv.x > 1.0 || uv.y > 1.0 || ndc.z > 1.0 || ndc.z < 0.0) {
        return 1.0;
    }
    return textureSampleCompareLevel(shadow_map, shadow_sampler, uv, ndc.z - 0.001);
}

@fragment
fn fs_ocean(in: OceanOut) -> SeaFsOut {
    let p = in.world;
    let r = in.rest;
    let v = view_dir(p);
    let dist = length(g.eye.xyz - p);
    let t = sea.center.w;
    // A pixel's footprint on the water (m): waves and ripples smaller than a few pixels are
    // faded out instead of sparkling.
    let fp = g.params.w * select(1.0, dist, g.eye.w > 0.5) / max(abs(v.y), 0.15);

    // The waves' normal (all six, wherever a pixel can show them) and how compressed the
    // surface is (crests).
    var nrm = vec3<f32>(0.0, 1.0, 0.0);
    var crest = 0.0;
    for (var i = 0u; i < 6u; i++) {
        let a = sea.wave_a[i];
        let b = sea.wave_b[i];
        let amp = a.w * smoothstep(1.5 * fp, 5.0 * fp, b.z);
        let ph = wrap_phase(a.z * dot(a.xy, r) + b.y);
        let s = sin(ph);
        let c = cos(ph);
        let wa = a.z * amp;
        nrm.x -= a.x * wa * c;
        nrm.z -= a.y * wa * c;
        nrm.y -= b.x * wa * s;
        crest += b.x * wa * s;
    }
    var n = normalize(nrm);
    let crest_n = crest / max(sea.misc.z, 1e-3);
    let height = (p.y - sea.center.z) / max(sea.misc.w, 1e-3);

    // Ripples: two octaves of scrolling noise slopes, fading with distance.
    let wd = sea.misc.xy;
    let ripple = sea.deep.w * (1.0 - smoothstep(0.08, 0.6, fp));
    var bump = vec2<f32>(0.0);
    var foam_noise = 0.5;
    if (ripple > 0.0) {
        let n1 = vnd(p.xz * 0.37 + wd * (t * 0.22));
        let n2 = vnd(vec2<f32>(p.x * 0.7 - p.z * 0.71, p.x * 0.71 + p.z * 0.7) * 1.1 + wd * (t * 0.5) + vec2<f32>(17.0, 3.0));
        bump = n1.yz * 0.37 + n2.yz * 1.1 * 0.55;
        foam_noise = n1.x * 0.6 + n2.x * 0.4;
    }
    // Rain rings: one expanding ring per 0.8 m cell.
    let rain = sea.refl_zenith.w * (1.0 - smoothstep(0.02, 0.15, fp));
    if (rain > 0.0) {
        let cell = 0.8;
        let q = p.xz / cell;
        let ci = vec2<i32>(floor(q));
        let h1 = sea_hash(ci);
        let h2 = sea_hash(ci + vec2<i32>(31, 7));
        let centre = (floor(q) + 0.25 + 0.5 * vec2<f32>(h1, h2)) * cell;
        let age = fract(t * 1.4 + h1 * 7.0);
        let rad = age * cell * 0.4;
        let dv = p.xz - centre;
        let dd = length(dv);
        let x = (dd - rad) / 0.035;
        let ring = exp(-x * x) * (1.0 - age) * rain;
        bump += dv / max(dd, 1e-3) * (ring * 2.5);
    }
    n = normalize(n - vec3<f32>(bump.x, 0.0, bump.y) * (0.3 * max(ripple, rain)));

    // Depth under the moving surface (surf follows the waves) and under the mean level (the
    // colour of the shallows does not, or storm troughs would paint sandbanks).
    let bed = seabed_at(p.xz);
    let depth = p.y - bed;
    let still = sea.center.z - bed;
    let l = -g.sun_dir.xyz;
    let sh = water_shadow(p);
    let ndv = max(dot(n, v), 0.0);
    let ndl = max(dot(n, l), 0.0);

    // The water's own colour: mid water seen from above, deep water toward the distance,
    // lighter on the crests; turquoise over the shallows.
    let steep = smoothstep(0.02, 0.6, v.y);
    var body = mix(sea.deep.rgb, sea.mid.rgb, steep * 0.75);
    body = mix(body, sea.mid.rgb * 1.1, clamp(height * 0.5 + 0.2, 0.0, 1.0) * 0.35);
    let shallow = 1.0 - smoothstep(0.4, 7.0, still);
    let sandy = sea.shallow.rgb * vec3<f32>(1.25, 1.15, 0.95) + vec3<f32>(0.05, 0.04, 0.0);
    body = mix(body, mix(sea.shallow.rgb, sandy, 1.0 - smoothstep(0.0, 1.4, still)), shallow);
    // Water is lit mostly by the sky: a shadow only takes away part of its light.
    let sun_amt = g.sun_color.rgb * mix(0.55, 1.0, sh);
    var col = body * (g.sky_color.rgb * 0.9 + sun_amt * (0.25 + 0.35 * ndl));
    // Light through the crests: turquoise, strongest looking toward the sun (less under a
    // stormy sky).
    let back = pow(max(dot(-v, l), 0.0), 3.0);
    let sss = clamp(height * 0.8 + crest_n * 0.6, 0.0, 1.0) * (1.0 - 0.7 * sea.horizon.w);
    col += sea.shallow.rgb * sun_amt * (sss * (0.08 + 0.55 * back));

    // Reflection (Fresnel) and the sun's glitter.
    let fres = 0.02 + 0.98 * pow(1.0 - ndv, 5.0);
    let refl = reflected_sky(reflect(-v, n));
    col = mix(col, refl, fres * 0.92);
    let hv = normalize(l + v);
    let nh = max(dot(n, hv), 0.0);
    let glint = (pow(nh, 900.0) * 32.0 + pow(nh, 90.0) * 0.3) * sea.shallow.w;
    col += g.sun_color.rgb * (glint * sh);

    // Foam: on the crests (more as the sea roughens), broken up by noise, and the surf along
    // the shore.
    let amount = sea.mid.w;
    // Break-up noise for the whitecaps (metres across; its average where pixels are bigger).
    let fb = mix(vn(p.xz * 0.3 + wd * (t * 0.08)) * 0.6 + vn(p.xz * 0.9 + vec2<f32>(5.0, 1.0)) * 0.4, 0.5, smoothstep(0.6, 2.5, fp));
    let breakup = (fb - 0.5) * 0.5 + (foam_noise - 0.5) * 0.2;
    let th = 1.0 - 0.85 * amount;
    var foam = smoothstep(th, th + 0.14, crest_n + breakup) * smoothstep(0.0, 0.05, amount);
    foam = max(foam, smoothstep(0.85, 1.0, height + breakup * 0.6) * amount * 0.7);
    // Rough seas: streaks of foam drawn out along the wind (their average far away).
    let streaky = smoothstep(0.35, 0.9, amount);
    if (streaky > 0.0) {
        let along = dot(p.xz, wd);
        let across = dot(p.xz, vec2<f32>(-wd.y, wd.x));
        let sharp = 1.0 - smoothstep(0.25, 1.2, fp);
        // Broken lines a few metres apart, running with the wind and wandering slowly.
        let q = across * 0.25 + vn(vec2<f32>(along * 0.012, across * 0.03)) * 3.0;
        let line = 1.0 - smoothstep(0.03, 0.09, abs(fract(q) - 0.5));
        let broken = smoothstep(0.45, 0.72, vn(vec2<f32>(along * 0.05 - t * 0.1, across * 0.25 + 7.0)));
        let streak = mix(0.06, line * broken * 0.8, sharp);
        foam = max(foam, streak * streaky * (0.45 + 0.55 * clamp(crest_n * 2.0, 0.0, 1.0)));
    }
    // Surf: solid at the waterline, then lines of foam washing toward the shore.
    let band = sea.foam.w;
    if (band > 0.0 && depth < band) {
        let sd = max(depth, 0.0) / band;
        let lap = sin(sd * 12.0 - t * 1.7 + vn(p.xz * 0.25) * 6.0) * 0.5 + 0.5;
        let edge = 1.0 - smoothstep(0.04, 0.16, sd);
        let lines = smoothstep(0.74, 0.92, lap) * (1.0 - sd) * (1.0 - sd) * smoothstep(0.3, 0.6, foam_noise + vn(p.xz * 1.3) * 0.5);
        foam = max(foam, max(edge, lines * 0.9));
    }
    let foam_lit = sea.foam.rgb * (g.sky_color.rgb * 0.9 + sun_amt * (0.45 + 0.4 * max(l.y, 0.0)));
    col = mix(col, foam_lit, clamp(foam, 0.0, 1.0));
    col = apply_fog(p, col);

    var o: SeaFsOut;
    o.color = vec4<f32>(col, 1.0);
    // Group 0: no outlines on the water itself (its far crests would draw lines); what stands
    // in it still gets its outline against the water.
    o.normal = vec4<f32>(n, 0.0);
    return o;
}

// ---------------------------------------------------------------- rain

struct RainOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) along: f32,
    @location(1) @interpolate(flat) alpha: f32,
};

// Streak `ii`: a drop in a box around the camera that falls with the wind and wraps around the
// box, drawn as a thin camera-facing quad stretched along its motion.
@vertex
fn vs_rain(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> RainOut {
    let box = vec3<f32>(30.0, 22.0, 30.0);
    let h = vec3<f32>(hash_u(ii * 3u), hash_u(ii * 3u + 1u), hash_u(ii * 3u + 2u));
    let fall = sea.rain.w * (0.85 + 0.3 * hash_u(ii * 7u + 5u));
    let vel = vec3<f32>(sea.rain.x, -fall, sea.rain.y);
    let base = g.eye.xyz + g.forward.xyz * 8.0 - box * 0.5;
    let rel = h * box + vel * g.params.x - base;
    let p = base + rel - box * floor(rel / box);
    let dir = normalize(vel);
    let len = 0.045 * length(vel);
    let to_eye = g.eye.xyz - p;
    let d = max(length(to_eye), 1e-3);
    var side = cross(dir, to_eye / d);
    side = side / max(length(side), 1e-4);
    let width = max(0.006, d * g.params.w * 0.75);
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0));
    let c = corners[vi];
    let wp = p - dir * (len * c.x) + side * (width * c.y);
    var o: RainOut;
    o.clip = g.view_proj * vec4<f32>(wp, 1.0);
    o.along = c.x;
    let fade = smoothstep(0.4, 1.5, d) * (1.0 - smoothstep(9.0, 15.0, d));
    o.alpha = clamp(sea.rain.z, 0.0, 1.0) * fade * min(0.006 / width, 1.0) * 0.28;
    return o;
}

@fragment
fn fs_rain(in: RainOut) -> SeaFsOut {
    let a = in.alpha * (1.0 - in.along * 0.7);
    var o: SeaFsOut;
    o.color = vec4<f32>(sea.rain_color.rgb * a, a);
    o.normal = vec4<f32>(0.0);
    return o;
}
