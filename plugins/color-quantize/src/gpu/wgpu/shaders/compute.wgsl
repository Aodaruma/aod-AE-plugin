const MAX_CLUSTERS: u32 = 64u;
const WORKGROUP_SIZE: u32 = 256u;
const TAU: f32 = 6.28318530717958647692;
const OKLAB_AB_MAX: f32 = 0.5;
const OKLCH_CHROMA_MAX: f32 = 0.4;
const YIQ_I_MAX: f32 = 0.5957;
const YIQ_Q_MAX: f32 = 0.5226;
const INVALID_LABEL: u32 = MAX_CLUSTERS;

struct Params {
    size: vec4<u32>,  // x: width, y: height, z: pixel_count, w: cluster_count
    mode: vec4<u32>,  // x: color_space, y: rgb_only
    scale: vec4<f32>, // x: sum_scale
}

struct PixelBuffer {
    data: array<vec4<f32>>,
}

struct CentroidBuffer {
    data: array<vec4<f32>, MAX_CLUSTERS>,
}

struct SumsBuffer {
    data: array<atomic<u32>, MAX_CLUSTERS * 4u>,
}

struct CountsBuffer {
    data: array<atomic<u32>, MAX_CLUSTERS>,
}

struct LabelBuffer {
    data: array<u32>,
}

struct ChangeCounterBuffer {
    value: atomic<u32>,
}

@group(0) @binding(0)
var<uniform> params: Params;

@group(0) @binding(1)
var<storage, read> input_pixels: PixelBuffer;

@group(0) @binding(2)
var<storage, read_write> centroids: CentroidBuffer;

@group(0) @binding(3)
var<storage, read_write> sums: SumsBuffer;

@group(0) @binding(4)
var<storage, read_write> counts: CountsBuffer;

@group(0) @binding(5)
var<storage, read_write> labels: LabelBuffer;

@group(0) @binding(6)
var<storage, read_write> change_counter: ChangeCounterBuffer;

@group(0) @binding(7)
var<storage, read_write> output_pixels: PixelBuffer;

var<workgroup> local_sums: array<atomic<u32>, MAX_CLUSTERS * 4u>;
var<workgroup> local_counts: array<atomic<u32>, MAX_CLUSTERS>;

fn clamp01(v: f32) -> f32 {
    return clamp(v, 0.0, 1.0);
}

fn linear_to_srgb_channel(v: f32) -> f32 {
    let x = clamp01(v);
    if x <= 0.0031308 {
        return 12.92 * x;
    }
    return 1.055 * pow(x, 1.0 / 2.4) - 0.055;
}

fn linear_to_srgb(rgb: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        linear_to_srgb_channel(rgb.x),
        linear_to_srgb_channel(rgb.y),
        linear_to_srgb_channel(rgb.z),
    );
}

fn encode_signed(value: f32, max_abs: f32) -> f32 {
    return clamp01((value / max_abs + 1.0) * 0.5);
}

fn decode_signed(value: f32, max_abs: f32) -> f32 {
    return (clamp01(value) * 2.0 - 1.0) * max_abs;
}

fn hue_distance(a: f32, b: f32) -> f32 {
    let d = abs(a - b);
    return min(d, 1.0 - d);
}

fn feature_distance(a: vec3<f32>, b: vec3<f32>, color_space: u32) -> f32 {
    var dx: f32;
    if color_space == 2u || color_space == 3u {
        dx = hue_distance(a.x, b.x);
    } else {
        dx = a.x - b.x;
    }
    let dy = a.y - b.y;
    let dz = a.z - b.z;
    return dx * dx + dy * dy + dz * dz;
}

fn nearest_centroid(sample: vec3<f32>) -> u32 {
    var best_idx: u32 = 0u;
    var best_dist: f32 = 1.0e30;
    var i: u32 = 0u;

    loop {
        if i >= params.size.w {
            break;
        }
        let center = centroids.data[i].xyz;
        let dist = feature_distance(sample, center, params.mode.x);
        if dist < best_dist {
            best_dist = dist;
            best_idx = i;
        }
        i = i + 1u;
    }

    return best_idx;
}

fn oklab_to_rgb(oklab: vec3<f32>) -> vec3<f32> {
    let l_ = oklab.x + 0.3963377774 * oklab.y + 0.2158037573 * oklab.z;
    let m_ = oklab.x - 0.1055613458 * oklab.y - 0.0638541728 * oklab.z;
    let s_ = oklab.x - 0.0894841775 * oklab.y - 1.2914855480 * oklab.z;

    let l = l_ * l_ * l_;
    let m = m_ * m_ * m_;
    let s = s_ * s_ * s_;

    return vec3<f32>(
        4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
        -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
        -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s
    );
}

fn hsv_to_rgb(hsv: vec3<f32>) -> vec3<f32> {
    let h = fract(clamp01(hsv.x));
    let s = clamp01(hsv.y);
    let v = clamp01(hsv.z);

    if s <= 1.0e-8 {
        return vec3<f32>(v, v, v);
    }

    let h6 = h * 6.0;
    let i = u32(floor(h6));
    let f = h6 - floor(h6);
    let p = v * (1.0 - s);
    let q = v * (1.0 - f * s);
    let t = v * (1.0 - (1.0 - f) * s);

    switch i % 6u {
        case 0u: { return vec3<f32>(v, t, p); }
        case 1u: { return vec3<f32>(q, v, p); }
        case 2u: { return vec3<f32>(p, v, t); }
        case 3u: { return vec3<f32>(p, q, v); }
        case 4u: { return vec3<f32>(t, p, v); }
        default: { return vec3<f32>(v, p, q); }
    }
}

fn decode_feature_to_rgb(feature: vec3<f32>, color_space: u32) -> vec3<f32> {
    if color_space == 1u {
        let l = clamp01(feature.z);
        let a = decode_signed(feature.x, OKLAB_AB_MAX);
        let b = decode_signed(feature.y, OKLAB_AB_MAX);
        let lin = clamp(oklab_to_rgb(vec3<f32>(l, a, b)), vec3<f32>(0.0), vec3<f32>(1.0));
        return clamp(linear_to_srgb(lin), vec3<f32>(0.0), vec3<f32>(1.0));
    }

    if color_space == 2u {
        let l = clamp01(feature.z);
        let c = clamp01(feature.y) * OKLCH_CHROMA_MAX;
        let hue = clamp01(feature.x) * TAU;
        let a = c * cos(hue);
        let b = c * sin(hue);
        let lin = clamp(oklab_to_rgb(vec3<f32>(l, a, b)), vec3<f32>(0.0), vec3<f32>(1.0));
        return clamp(linear_to_srgb(lin), vec3<f32>(0.0), vec3<f32>(1.0));
    }

    if color_space == 3u {
        return clamp(hsv_to_rgb(feature), vec3<f32>(0.0), vec3<f32>(1.0));
    }

    if color_space == 4u {
        let y = clamp01(feature.z);
        let i = decode_signed(feature.x, YIQ_I_MAX);
        let q = decode_signed(feature.y, YIQ_Q_MAX);
        let r = y + 0.9563 * i + 0.6210 * q;
        let g = y - 0.2721 * i - 0.6474 * q;
        let b = y - 1.1070 * i + 1.7046 * q;
        return clamp(vec3<f32>(r, g, b), vec3<f32>(0.0), vec3<f32>(1.0));
    }

    return clamp(linear_to_srgb(feature), vec3<f32>(0.0), vec3<f32>(1.0));
}

@compute @workgroup_size(WORKGROUP_SIZE)
fn clear_labels(@builtin(global_invocation_id) gid: vec3<u32>) {
    let idx = gid.x;
    if idx >= params.size.z {
        return;
    }
    labels.data[idx] = INVALID_LABEL;
}

@compute @workgroup_size(WORKGROUP_SIZE)
fn assign_accumulate(
    @builtin(global_invocation_id) gid: vec3<u32>,
    @builtin(local_invocation_id) lid: vec3<u32>,
) {
    var i = lid.x;
    loop {
        if i >= MAX_CLUSTERS * 4u {
            break;
        }
        atomicStore(&local_sums[i], 0u);
        i = i + WORKGROUP_SIZE;
    }

    var j = lid.x;
    loop {
        if j >= MAX_CLUSTERS {
            break;
        }
        atomicStore(&local_counts[j], 0u);
        j = j + WORKGROUP_SIZE;
    }

    workgroupBarrier();

    let idx = gid.x;
    if idx < params.size.z {
        let sample = input_pixels.data[idx];
        let centroid_idx = nearest_centroid(sample.xyz);
        let old = labels.data[idx];
        labels.data[idx] = centroid_idx;
        if old != centroid_idx {
            atomicAdd(&change_counter.value, 1u);
        }

        let base = centroid_idx * 4u;
        let scale = params.scale.x;
        atomicAdd(&local_sums[base + 0u], u32(round(clamp01(sample.x) * scale)));
        atomicAdd(&local_sums[base + 1u], u32(round(clamp01(sample.y) * scale)));
        atomicAdd(&local_sums[base + 2u], u32(round(clamp01(sample.z) * scale)));
        atomicAdd(&local_sums[base + 3u], u32(round(clamp01(sample.w) * scale)));
        atomicAdd(&local_counts[centroid_idx], 1u);
    }

    workgroupBarrier();

    var k = lid.x;
    loop {
        if k >= MAX_CLUSTERS * 4u {
            break;
        }
        let v = atomicLoad(&local_sums[k]);
        if v != 0u {
            atomicAdd(&sums.data[k], v);
        }
        k = k + WORKGROUP_SIZE;
    }

    var c = lid.x;
    loop {
        if c >= MAX_CLUSTERS {
            break;
        }
        let v = atomicLoad(&local_counts[c]);
        if v != 0u {
            atomicAdd(&counts.data[c], v);
        }
        c = c + WORKGROUP_SIZE;
    }
}

@compute @workgroup_size(64)
fn update_centroids(@builtin(global_invocation_id) gid: vec3<u32>) {
    let centroid_idx = gid.x;
    if centroid_idx >= params.size.w {
        return;
    }

    let count = atomicLoad(&counts.data[centroid_idx]);
    if count == 0u {
        return;
    }

    let base = centroid_idx * 4u;
    let inv = 1.0 / (f32(count) * params.scale.x);

    let x = f32(atomicLoad(&sums.data[base + 0u])) * inv;
    let y = f32(atomicLoad(&sums.data[base + 1u])) * inv;
    let z = f32(atomicLoad(&sums.data[base + 2u])) * inv;
    let w = f32(atomicLoad(&sums.data[base + 3u])) * inv;

    centroids.data[centroid_idx] = vec4<f32>(clamp01(x), clamp01(y), clamp01(z), clamp01(w));
}

@compute @workgroup_size(WORKGROUP_SIZE)
fn write_output(@builtin(global_invocation_id) gid: vec3<u32>) {
    let idx = gid.x;
    if idx >= params.size.z {
        return;
    }

    let src = input_pixels.data[idx];
    let label = labels.data[idx];
    var centroid_idx: u32 = 0u;
    if label < params.size.w {
        centroid_idx = label;
    }
    let centroid = centroids.data[centroid_idx];
    let rgb = decode_feature_to_rgb(centroid.xyz, params.mode.x);
    let alpha = select(centroid.w, src.w, params.mode.y != 0u);

    output_pixels.data[idx] = vec4<f32>(
        clamp01(rgb.x),
        clamp01(rgb.y),
        clamp01(rgb.z),
        clamp01(alpha)
    );
}
