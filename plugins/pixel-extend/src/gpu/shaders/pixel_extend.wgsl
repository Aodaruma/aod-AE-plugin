const EPSILON: f32 = 0.000001;

struct Params {
    image: vec4<u32>,
    region: vec4<u32>,
    counts: vec4<u32>,
    flags: vec4<u32>,
    direction: vec4<f32>,
    sampling: vec4<f32>,
}

struct PathStep {
    geometry: vec4<f32>,
    metadata: vec4<f32>,
}

struct ExpansionSample {
    values: vec4<f32>,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> source_pixels: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> source_masks: array<f32>;
@group(0) @binding(3) var<storage, read> directions: array<vec2<f32>>;
@group(0) @binding(4) var<storage, read> path_steps: array<PathStep>;
@group(0) @binding(5) var<storage, read> expansion_samples: array<ExpansionSample>;
@group(0) @binding(6) var<storage, read_write> output_pixels: array<vec4<f32>>;

fn safe_unit(value: f32) -> f32 {
    if value != value {
        return 0.0;
    }
    return clamp(value, 0.0, 1.0);
}

fn sanitize_pixel(pixel: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(
        safe_unit(pixel.r),
        safe_unit(pixel.g),
        safe_unit(pixel.b),
        safe_unit(pixel.a),
    );
}

fn transparent_pixel() -> vec4<f32> {
    return vec4<f32>(0.0);
}

fn pixel_index(x: i32, y: i32) -> u32 {
    return u32(y) * params.image.x + u32(x);
}

fn in_bounds(x: i32, y: i32) -> bool {
    return x >= 0 && y >= 0 && x < i32(params.image.x) && y < i32(params.image.y);
}

fn fetch_pixel(x: i32, y: i32) -> vec4<f32> {
    if !in_bounds(x, y) {
        return transparent_pixel();
    }
    return source_pixels[pixel_index(x, y)];
}

fn fetch_mask(x: i32, y: i32) -> f32 {
    if !in_bounds(x, y) {
        return 0.0;
    }
    return source_masks[pixel_index(x, y)];
}

fn round_away_from_zero(value: f32) -> i32 {
    if value >= 0.0 {
        return i32(floor(value + 0.5));
    }
    return i32(ceil(value - 0.5));
}

fn cubic_weight(distance: f32) -> f32 {
    let x = abs(distance);
    let a = -0.5;
    if x <= 1.0 {
        return (a + 2.0) * x * x * x - (a + 3.0) * x * x + 1.0;
    }
    if x < 2.0 {
        return a * x * x * x - 5.0 * a * x * x + 8.0 * a * x - 4.0 * a;
    }
    return 0.0;
}

fn mitchell_weight(distance: f32) -> f32 {
    let x = abs(distance);
    let b = params.sampling.x;
    let c = params.sampling.y;
    if x < 1.0 {
        return ((12.0 - 9.0 * b - 6.0 * c) * x * x * x
            + (-18.0 + 12.0 * b + 6.0 * c) * x * x
            + (6.0 - 2.0 * b)) / 6.0;
    }
    if x < 2.0 {
        return ((-b - 6.0 * c) * x * x * x
            + (6.0 * b + 30.0 * c) * x * x
            + (-12.0 * b - 48.0 * c) * x
            + (8.0 * b + 24.0 * c)) / 6.0;
    }
    return 0.0;
}

fn interpolation_weight(distance: f32) -> f32 {
    if params.image.z == 2u {
        return cubic_weight(distance);
    }
    return mitchell_weight(distance);
}

fn sample_mask_4x4(position: vec2<f32>) -> f32 {
    let start = vec2<i32>(floor(position)) - vec2<i32>(1);
    var sum = 0.0;
    var weight_sum = 0.0;
    for (var oy = 0i; oy < 4i; oy += 1i) {
        let sy = start.y + oy;
        let wy = interpolation_weight(position.y - f32(sy));
        for (var ox = 0i; ox < 4i; ox += 1i) {
            let sx = start.x + ox;
            let weight = interpolation_weight(position.x - f32(sx)) * wy;
            sum += fetch_mask(sx, sy) * weight;
            weight_sum += weight;
        }
    }
    if abs(weight_sum) <= EPSILON {
        return 0.0;
    }
    return sum / weight_sum;
}

fn sample_pixel_4x4(position: vec2<f32>) -> vec4<f32> {
    let start = vec2<i32>(floor(position)) - vec2<i32>(1);
    var sum = transparent_pixel();
    var weight_sum = 0.0;
    for (var oy = 0i; oy < 4i; oy += 1i) {
        let sy = start.y + oy;
        let wy = interpolation_weight(position.y - f32(sy));
        for (var ox = 0i; ox < 4i; ox += 1i) {
            let sx = start.x + ox;
            let weight = interpolation_weight(position.x - f32(sx)) * wy;
            sum += fetch_pixel(sx, sy) * weight;
            weight_sum += weight;
        }
    }
    if abs(weight_sum) <= EPSILON {
        return transparent_pixel();
    }
    return clamp(sum / weight_sum, vec4<f32>(0.0), vec4<f32>(1.0));
}

fn sample_mask(position: vec2<f32>) -> f32 {
    if params.image.z == 0u {
        return fetch_mask(round_away_from_zero(position.x), round_away_from_zero(position.y));
    }
    if params.image.z >= 2u {
        return sample_mask_4x4(position);
    }

    let lower = vec2<i32>(floor(position));
    let fraction = position - vec2<f32>(lower);
    let top = mix(fetch_mask(lower.x, lower.y), fetch_mask(lower.x + 1i, lower.y), fraction.x);
    let bottom = mix(
        fetch_mask(lower.x, lower.y + 1i),
        fetch_mask(lower.x + 1i, lower.y + 1i),
        fraction.x,
    );
    return mix(top, bottom, fraction.y);
}

fn sample_pixel(position: vec2<f32>) -> vec4<f32> {
    if params.image.z == 0u {
        return fetch_pixel(round_away_from_zero(position.x), round_away_from_zero(position.y));
    }
    if params.image.z >= 2u {
        return sample_pixel_4x4(position);
    }

    let lower = vec2<i32>(floor(position));
    let fraction = position - vec2<f32>(lower);
    let top = mix(fetch_pixel(lower.x, lower.y), fetch_pixel(lower.x + 1i, lower.y), fraction.x);
    let bottom = mix(
        fetch_pixel(lower.x, lower.y + 1i),
        fetch_pixel(lower.x + 1i, lower.y + 1i),
        fraction.x,
    );
    return mix(top, bottom, fraction.y);
}

fn alpha_over(top: vec4<f32>, bottom: vec4<f32>) -> vec4<f32> {
    let top_alpha = clamp(top.a, 0.0, 1.0);
    let bottom_alpha = clamp(bottom.a, 0.0, 1.0);
    let output_alpha = top_alpha + bottom_alpha * (1.0 - top_alpha);
    if output_alpha <= EPSILON {
        return transparent_pixel();
    }
    return vec4<f32>(top.rgb + bottom.rgb * (1.0 - top_alpha), output_alpha);
}

fn offset_point(
    output_position: vec2<f32>,
    direction: vec2<f32>,
    local_position: vec2<f32>,
) -> vec2<f32> {
    return output_position + vec2<f32>(
        direction.x * local_position.x - direction.y * local_position.y,
        direction.y * local_position.x + direction.x * local_position.y,
    );
}

fn sample_extension_point(position: vec2<f32>, falloff: f32) -> vec4<f32> {
    let mask = clamp(sample_mask(position), 0.0, 1.0);
    let opacity = clamp(mask * falloff, 0.0, 1.0);
    if opacity <= EPSILON {
        return transparent_pixel();
    }
    return sample_pixel(position) * opacity;
}

fn extend_path(
    output_position: vec2<f32>,
    direction: vec2<f32>,
    path_index: u32,
) -> vec4<f32> {
    var result = transparent_pixel();
    let first_step = path_index * params.counts.y;
    for (var step_index = 0u; step_index < params.counts.y; step_index += 1u) {
        let step = path_steps[first_step + step_index];
        var step_pixel = transparent_pixel();
        if step.geometry.z <= EPSILON || params.counts.z <= 1u {
            let position = offset_point(output_position, direction, step.geometry.xy);
            step_pixel = sample_extension_point(position, step.geometry.w);
        } else {
            for (var sample_index = 0u; sample_index < params.counts.z; sample_index += 1u) {
                let sample = expansion_samples[sample_index].values;
                let local_y = step.geometry.y
                    + step.metadata.x * sample.x * step.geometry.z;
                let position = offset_point(
                    output_position,
                    direction,
                    vec2<f32>(step.geometry.x, local_y),
                );
                let pixel = sample_extension_point(position, step.geometry.w * sample.y);
                step_pixel = alpha_over(step_pixel, pixel);
                if step_pixel.a >= 0.999 {
                    break;
                }
            }
        }

        if step_pixel.a > EPSILON {
            result = alpha_over(result, step_pixel);
            if result.a >= 0.999 {
                break;
            }
        }
    }
    return result;
}

fn unpremultiplied_rgb(pixel: vec4<f32>) -> vec3<f32> {
    if pixel.a <= EPSILON {
        return vec3<f32>(0.0);
    }
    return vec3<f32>(
        safe_unit(pixel.r / pixel.a),
        safe_unit(pixel.g / pixel.a),
        safe_unit(pixel.b / pixel.a),
    );
}

fn blend_channel(base: f32, blend: f32) -> f32 {
    if params.counts.w == 1u {
        return clamp(base + blend, 0.0, 1.0);
    }
    if params.counts.w == 2u {
        return base * blend;
    }
    if params.counts.w == 3u {
        return 1.0 - (1.0 - base) * (1.0 - blend);
    }
    if params.counts.w == 4u {
        if base <= 0.5 {
            return 2.0 * base * blend;
        }
        return 1.0 - 2.0 * (1.0 - base) * (1.0 - blend);
    }
    if params.counts.w == 5u {
        return min(base, blend);
    }
    if params.counts.w == 6u {
        return max(base, blend);
    }
    return blend;
}

fn blend_behind_source(source: vec4<f32>, extension: vec4<f32>) -> vec4<f32> {
    let base = unpremultiplied_rgb(source);
    let blend = unpremultiplied_rgb(extension);
    let blended = vec4<f32>(
        blend_channel(base.r, blend.r) * extension.a,
        blend_channel(base.g, blend.g) * extension.a,
        blend_channel(base.b, blend.b) * extension.a,
        extension.a,
    );
    return alpha_over(blended, source);
}

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if invocation.x >= params.image.x || invocation.y >= params.image.y {
        return;
    }

    let index = invocation.y * params.image.x + invocation.x;
    let source = source_pixels[index];
    var extension = transparent_pixel();
    if invocation.x >= params.region.x
        && invocation.y >= params.region.y
        && invocation.x < params.region.z
        && invocation.y < params.region.w
    {
        var direction = params.direction.xy;
        if params.image.w != 0u {
            let direction_index = (invocation.y - params.region.y)
                * (params.region.z - params.region.x)
                + (invocation.x - params.region.x);
            direction = directions[direction_index];
        }
        let output_position = vec2<f32>(invocation.xy);
        for (var path_index = 0u; path_index < params.counts.x; path_index += 1u) {
            extension = alpha_over(
                extension,
                extend_path(output_position, direction, path_index),
            );
        }
        extension *= clamp(params.sampling.z, 0.0, 1.0);
    }

    var output = extension;
    if params.flags.x != 0u {
        output = blend_behind_source(source, extension);
    }
    output_pixels[index] = sanitize_pixel(output);
}
