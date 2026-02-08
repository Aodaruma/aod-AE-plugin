struct Params {
    size: vec4<u32>, // x=width, y=height
    mode: vec4<u32>, // x=inverse
};

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> in_real: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> in_imag: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read_write> out_real: array<vec4<f32>>;
@group(0) @binding(4) var<storage, read_write> out_imag: array<vec4<f32>>;

const TAU: f32 = 6.28318530717958647692;

fn cmul(a_re: f32, a_im: f32, b_re: f32, b_im: f32) -> vec2<f32> {
    return vec2<f32>(a_re * b_re - a_im * b_im, a_re * b_im + a_im * b_re);
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let width = params.size.x;
    let height = params.size.y;
    if (gid.x >= width || gid.y >= height) {
        return;
    }

    let norm = f32(width * height);
    let xk = f32(gid.x);
    let yk = f32(gid.y);
    let sign = select(-1.0, 1.0, params.mode.x != 0u);

    var sum_re = vec4<f32>(0.0);
    var sum_im = vec4<f32>(0.0);

    var ny: u32 = 0u;
    loop {
        if (ny >= height) {
            break;
        }

        var nx: u32 = 0u;
        loop {
            if (nx >= width) {
                break;
            }

            let idx = ny * width + nx;
            let src_re = in_real[idx];
            let src_im = in_imag[idx];

            let phase = TAU * ((xk * f32(nx)) / f32(width) + (yk * f32(ny)) / f32(height));
            let c = cos(phase);
            let s = sin(phase) * sign;

            let r = cmul(src_re.x, src_im.x, c, s);
            sum_re.x += r.x;
            sum_im.x += r.y;

            let g = cmul(src_re.y, src_im.y, c, s);
            sum_re.y += g.x;
            sum_im.y += g.y;

            let b = cmul(src_re.z, src_im.z, c, s);
            sum_re.z += b.x;
            sum_im.z += b.y;

            let a = cmul(src_re.w, src_im.w, c, s);
            sum_re.w += a.x;
            sum_im.w += a.y;

            nx += 1u;
        }

        ny += 1u;
    }

    if (params.mode.x != 0u) {
        sum_re = sum_re / norm;
        sum_im = sum_im / norm;
    }

    let out_idx = gid.y * width + gid.x;
    out_real[out_idx] = sum_re;
    out_imag[out_idx] = sum_im;
}
