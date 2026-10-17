// Shared by the scene shader (common + materials + scene) and the sea and sky shader (common +
// sea): the frame's globals, the sun's shadow map and the fog.
//
// Depth is reversed: 1 at the near plane, 0 at the far plane (precise out to the horizon).

struct Globals {
    view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    light_view_proj: mat4x4<f32>,
    eye: vec4<f32>,          // xyz camera position; w = 1 perspective, 0 orthographic
    forward: vec4<f32>,      // xyz camera forward
    sun_dir: vec4<f32>,      // xyz direction light travels; w = shadows enabled
    sun_color: vec4<f32>,
    sky_color: vec4<f32>,
    ground_color: vec4<f32>,
    cut: vec4<f32>,          // xyz focus; w = cut height
    cut2: vec4<f32>,         // x cut radius, y height cut on, z fade radius, w fade on
    params: vec4<f32>,       // x time, y point light count, z shadow texel (world), w pixel footprint:
                             // metres per pixel at 1 m (perspective) or in all (orthographic)
    style: vec4<f32>,        // x flat shadow, y cel bands, z rim, w specular
    viewport: vec4<f32>,     // w, h, 1/w, 1/h
    fog: vec4<f32>,          // rgb colour, w = enabled
    fog2: vec4<f32>,         // x,y centre (xz), z start, w end
    cut3: vec4<f32>,         // x front cut distance, y front cut on
    wind: vec4<f32>,         // xy direction (world x, z), z strength, w gust speed
    push: vec4<f32>,         // xyz the pusher's feet (grass bends away), w radius (0 = none)
    fog3: vec4<f32>,         // x 1 = fog along the view ray, y height falloff (1/m), z base height, w sun glow
};

struct PointLight {
    pos_radius: vec4<f32>,
    color: vec4<f32>,
};

@group(0) @binding(0) var<uniform> g: Globals;
@group(0) @binding(1) var<storage, read> point_lights: array<PointLight>;
@group(0) @binding(2) var shadow_map: texture_depth_2d;
@group(0) @binding(3) var shadow_sampler: sampler_comparison;
@group(0) @binding(4) var point_shadow: texture_depth_2d_array;
@group(0) @binding(5) var<storage, read> point_mats: array<mat4x4<f32>>;

fn shadow_factor(p: vec3<f32>, n: vec3<f32>) -> f32 {
    if (g.sun_dir.w < 0.5) {
        return 1.0;
    }
    let texel = g.params.z;
    let lp = g.light_view_proj * vec4<f32>(p + n * texel * 1.5, 1.0);
    let ndc = lp.xyz / lp.w;
    let uv = vec2<f32>(ndc.x * 0.5 + 0.5, -ndc.y * 0.5 + 0.5);
    if (uv.x < 0.0 || uv.y < 0.0 || uv.x > 1.0 || uv.y > 1.0 || ndc.z > 1.0 || ndc.z < 0.0) {
        return 1.0;
    }
    let ts = 1.0 / f32(textureDimensions(shadow_map).x);
    let d = ndc.z - 0.0008;
    var s = 0.0;
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            s += textureSampleCompareLevel(shadow_map, shadow_sampler, uv + vec2<f32>(f32(x), f32(y)) * ts, d);
        }
    }
    return s / 9.0;
}

fn view_dir(p: vec3<f32>) -> vec3<f32> {
    if (g.eye.w > 0.5) {
        return normalize(g.eye.xyz - p);
    }
    return -g.forward.xyz;
}

// How much fog covers the point `p` (0..1). The horizontal mode measures the distance from the
// fog's centre across the ground; the ray mode measures the distance from the eye, weighted by
// how dense the air is along the way (it thins exponentially above the base height).
fn fog_amount(p: vec3<f32>) -> f32 {
    if (g.fog.w < 0.5) {
        return 0.0;
    }
    if (g.fog3.x < 0.5) {
        let d = length(p.xz - g.fog2.xy);
        return smoothstep(g.fog2.z, g.fog2.w, d);
    }
    let e = g.eye.xyz;
    let d = length(p - e);
    let k = g.fog3.y;
    var dens = 1.0;
    if (k > 0.0) {
        let h0 = max(e.y - g.fog3.z, 0.0);
        let h1 = max(p.y - g.fog3.z, 0.0);
        let dh = (h1 - h0) * k;
        if (abs(dh) > 1e-3) {
            dens = (exp(-k * h0) - exp(-k * h1)) / dh;
        } else {
            dens = exp(-k * h0);
        }
    }
    return smoothstep(g.fog2.z, g.fog2.w, d * dens);
}

// The fog's colour seen along direction `rd` (unit, from the eye): in ray mode sunlight
// scattered in the air brightens it toward the sun.
fn fog_tint(rd: vec3<f32>) -> vec3<f32> {
    if (g.fog3.x < 0.5 || g.fog3.w <= 0.0) {
        return g.fog.rgb;
    }
    let cs = max(dot(rd, -g.sun_dir.xyz), 0.0);
    return g.fog.rgb + g.sun_color.rgb * (pow(cs, 8.0) * g.fog3.w);
}

fn apply_fog(p: vec3<f32>, col: vec3<f32>) -> vec3<f32> {
    if (g.fog.w < 0.5) {
        return col;
    }
    return mix(col, fog_tint(-view_dir(p)), fog_amount(p));
}
