const PI: f32 = 3.14159265358979323846;
const TAU: f32 = 6.28318530717958647692;
const GABOR_IMPULSES_COUNT: i32 = 8;

struct Params {
    size: vec4<u32>,
    seed: vec4<u32>,
    core: vec4<f32>,
    orient: vec4<f32>,
    misc: vec4<f32>,
};

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read_write> out_buf: array<vec4<f32>>;

fn hash_u32(mut x: u32) -> u32 {
    x = x ^ (x >> 16u);
    x = x * 0x7FEB352Du;
    x = x ^ (x >> 15u);
    x = x * 0x846CA68Bu;
    x = x ^ (x >> 16u);
    return x;
}

fn hash_2d(cell_x: i32, cell_y: i32, impulse: i32, channel: u32, seed: u32) -> u32 {
    var h = seed ^ 0x9E3779B9u;
    h = h + bitcast<u32>(cell_x) * 0x85EBCA6Bu;
    h = h + bitcast<u32>(cell_y) * 0xC2B2AE35u;
    h = h + bitcast<u32>(impulse) * 0x27D4EB2Du;
    h = h + channel * 0x165667B1u;
    return hash_u32(h);
}

fn hash_3d(cell_x: i32, cell_y: i32, cell_z: i32, impulse: i32, channel: u32, seed: u32) -> u32 {
    var h = seed ^ 0x517CC1B7u;
    h = h + bitcast<u32>(cell_x) * 0x85EBCA6Bu;
    h = h + bitcast<u32>(cell_y) * 0xC2B2AE35u;
    h = h + bitcast<u32>(cell_z) * 0x9E3779B9u;
    h = h + bitcast<u32>(impulse) * 0x27D4EB2Du;
    h = h + channel * 0x165667B1u;
    return hash_u32(h);
}

fn rand01(h: u32) -> f32 {
    return f32(h) / 4294967295.0;
}

fn normalize3(v: vec3<f32>) -> vec3<f32> {
    let len2 = dot(v, v);
    if (len2 <= 1e-20) {
        return vec3<f32>(1.0, 0.0, 0.0);
    }
    return v * inverseSqrt(len2);
}

fn orientation_3d(azimuth: f32, elevation: f32) -> vec3<f32> {
    let cos_e = cos(elevation);
    return normalize3(vec3<f32>(
        cos_e * cos(azimuth),
        cos_e * sin(azimuth),
        sin(elevation)
    ));
}

fn compute_3d_orientation(
    cell_x: i32,
    cell_y: i32,
    cell_z: i32,
    impulse: i32,
    isotropy: f32,
    base_orientation: vec3<f32>,
    seed: u32
) -> vec3<f32> {
    if (isotropy <= 1e-6) {
        return base_orientation;
    }

    var inclination = acos(clamp(base_orientation.z, -1.0, 1.0));
    var azimuth = atan2(base_orientation.y, base_orientation.x);

    let random_inclination = rand01(hash_3d(cell_x, cell_y, cell_z, impulse, 0u, seed)) * PI;
    let random_azimuth = rand01(hash_3d(cell_x, cell_y, cell_z, impulse, 1u, seed)) * PI;

    inclination = inclination + random_inclination * isotropy;
    azimuth = azimuth + random_azimuth * isotropy;

    return normalize3(vec3<f32>(
        sin(inclination) * cos(azimuth),
        sin(inclination) * sin(azimuth),
        cos(inclination)
    ));
}

fn gabor_kernel_2d(dx: f32, dy: f32, frequency: f32, orientation: f32, radius2: f32) -> vec2<f32> {
    let hann = 0.5 + 0.5 * cos(PI * radius2);
    let gaussian = exp(-PI * radius2);
    let envelope = gaussian * hann;
    let dir = vec2<f32>(cos(orientation), sin(orientation));
    let angle = TAU * frequency * dot(vec2<f32>(dx, dy), dir);
    return envelope * vec2<f32>(cos(angle), sin(angle));
}

fn gabor_kernel_3d(
    dx: f32,
    dy: f32,
    dz: f32,
    frequency: f32,
    orientation: vec3<f32>,
    radius2: f32
) -> vec2<f32> {
    let hann = 0.5 + 0.5 * cos(PI * radius2);
    let gaussian = exp(-PI * radius2);
    let envelope = gaussian * hann;
    let angle = TAU * frequency * dot(vec3<f32>(dx, dy, dz), orientation);
    return envelope * vec2<f32>(cos(angle), sin(angle));
}

fn gabor_2d_cell(
    cell_x: i32,
    cell_y: i32,
    position: vec2<f32>,
    frequency: f32,
    isotropy: f32,
    base_orientation: f32,
    seed: u32
) -> vec2<f32> {
    var sum = vec2<f32>(0.0, 0.0);

    for (var impulse: i32 = 0; impulse < GABOR_IMPULSES_COUNT; impulse = impulse + 1) {
        let random_orientation = (rand01(hash_2d(cell_x, cell_y, impulse, 0u, seed)) - 0.5) * PI;
        let orientation = base_orientation + random_orientation * isotropy;

        let center_x = rand01(hash_2d(cell_x, cell_y, impulse, 1u, seed));
        let center_y = rand01(hash_2d(cell_x, cell_y, impulse, 2u, seed));

        let dx = position.x - center_x;
        let dy = position.y - center_y;
        let radius2 = dx * dx + dy * dy;

        if (radius2 >= 1.0) {
            continue;
        }

        let weight = select(-1.0, 1.0, rand01(hash_2d(cell_x, cell_y, impulse, 3u, seed)) >= 0.5);
        sum = sum + weight * gabor_kernel_2d(dx, dy, frequency, orientation, radius2);
    }

    return sum;
}

fn gabor_3d_cell(
    cell_x: i32,
    cell_y: i32,
    cell_z: i32,
    position: vec3<f32>,
    frequency: f32,
    isotropy: f32,
    base_orientation: vec3<f32>,
    seed: u32
) -> vec2<f32> {
    var sum = vec2<f32>(0.0, 0.0);

    for (var impulse: i32 = 0; impulse < GABOR_IMPULSES_COUNT; impulse = impulse + 1) {
        let orientation = compute_3d_orientation(
            cell_x,
            cell_y,
            cell_z,
            impulse,
            isotropy,
            base_orientation,
            seed
        );

        let center_x = rand01(hash_3d(cell_x, cell_y, cell_z, impulse, 2u, seed));
        let center_y = rand01(hash_3d(cell_x, cell_y, cell_z, impulse, 3u, seed));
        let center_z = rand01(hash_3d(cell_x, cell_y, cell_z, impulse, 4u, seed));

        let dx = position.x - center_x;
        let dy = position.y - center_y;
        let dz = position.z - center_z;
        let radius2 = dx * dx + dy * dy + dz * dz;

        if (radius2 >= 1.0) {
            continue;
        }

        let weight = select(-1.0, 1.0, rand01(hash_3d(cell_x, cell_y, cell_z, impulse, 5u, seed)) >= 0.5);
        sum = sum + weight * gabor_kernel_3d(dx, dy, dz, frequency, orientation, radius2);
    }

    return sum;
}

fn gabor_2d(
    coordinates: vec2<f32>,
    frequency: f32,
    isotropy: f32,
    base_orientation: f32,
    seed: u32
) -> vec2<f32> {
    let cell_x = i32(floor(coordinates.x));
    let cell_y = i32(floor(coordinates.y));
    let local_x = coordinates.x - f32(cell_x);
    let local_y = coordinates.y - f32(cell_y);

    var sum = vec2<f32>(0.0, 0.0);

    for (var j: i32 = -1; j <= 1; j = j + 1) {
        for (var i: i32 = -1; i <= 1; i = i + 1) {
            let current_x = cell_x + i;
            let current_y = cell_y + j;
            let position = vec2<f32>(local_x - f32(i), local_y - f32(j));
            sum = sum + gabor_2d_cell(
                current_x,
                current_y,
                position,
                frequency,
                isotropy,
                base_orientation,
                seed
            );
        }
    }

    return sum;
}

fn gabor_3d(
    coordinates: vec3<f32>,
    frequency: f32,
    isotropy: f32,
    base_orientation: vec3<f32>,
    seed: u32
) -> vec2<f32> {
    let cell_x = i32(floor(coordinates.x));
    let cell_y = i32(floor(coordinates.y));
    let cell_z = i32(floor(coordinates.z));
    let local_x = coordinates.x - f32(cell_x);
    let local_y = coordinates.y - f32(cell_y);
    let local_z = coordinates.z - f32(cell_z);

    var sum = vec2<f32>(0.0, 0.0);

    for (var k: i32 = -1; k <= 1; k = k + 1) {
        for (var j: i32 = -1; j <= 1; j = j + 1) {
            for (var i: i32 = -1; i <= 1; i = i + 1) {
                let current_x = cell_x + i;
                let current_y = cell_y + j;
                let current_z = cell_z + k;
                let position = vec3<f32>(
                    local_x - f32(i),
                    local_y - f32(j),
                    local_z - f32(k)
                );
                sum = sum + gabor_3d_cell(
                    current_x,
                    current_y,
                    current_z,
                    position,
                    frequency,
                    isotropy,
                    base_orientation,
                    seed
                );
            }
        }
    }

    return sum;
}

fn stddev_2d() -> f32 {
    let integral = 0.25;
    let second_moment = 0.5;
    return sqrt(f32(GABOR_IMPULSES_COUNT) * second_moment * integral);
}

fn stddev_3d() -> f32 {
    let integral = 1.0 / (4.0 * sqrt(2.0));
    let second_moment = 0.5;
    return sqrt(f32(GABOR_IMPULSES_COUNT) * second_moment * integral);
}

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let out_w = params.size.x;
    let out_h = params.size.y;
    if (gid.x >= out_w || gid.y >= out_h) {
        return;
    }

    let span = max(1.0, min(f32(out_w), f32(out_h)));
    let coord_x = (f32(gid.x) + 0.5 - params.misc.x) / span;
    let coord_y = (f32(gid.y) + 0.5 - params.misc.y) / span;
    let coord_z = params.orient.z * params.orient.w;

    let scale = max(params.core.x, 0.001);
    let frequency = max(params.core.y, 0.001);
    let isotropy = clamp(params.core.z, 0.0, 1.0);

    var phasor = vec2<f32>(0.0, 0.0);
    var normalization = 1.0;

    if (params.size.z == 0u) {
        let scaled = vec2<f32>(coord_x * scale, coord_y * scale);
        phasor = gabor_2d(scaled, frequency, isotropy, params.core.w, params.seed.x);
        normalization = 6.0 * stddev_2d();
    } else {
        let base_orientation = orientation_3d(params.orient.x, params.orient.y);
        let scaled = vec3<f32>(coord_x * scale, coord_y * scale, coord_z * scale);
        phasor = gabor_3d(scaled, frequency, isotropy, base_orientation, params.seed.x);
        normalization = 6.0 * stddev_3d();
    }

    let phase = atan2(phasor.y, phasor.x);
    var value = (phasor.y / normalization) * 0.5 + 0.5;

    if (params.size.w == 1u) {
        value = (phase + PI) / TAU;
    } else if (params.size.w == 2u) {
        value = length(phasor) / normalization;
    }

    value = value * params.misc.z + params.misc.w;

    let idx = gid.y * out_w + gid.x;
    out_buf[idx] = vec4<f32>(value, value, value, 1.0);
}
