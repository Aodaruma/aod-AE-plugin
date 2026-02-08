struct Params {
    size: vec4<u32>, // x=width, y=height, z=rank
};

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> u_buf: array<f32>;
@group(0) @binding(2) var<storage, read> v_buf: array<f32>;
@group(0) @binding(3) var<storage, read_write> out_buf: array<vec4<f32>>;

fn channel_value(ch: u32, x: u32, y: u32) -> f32 {
    let width = params.size.x;
    let height = params.size.y;
    let rank = params.size.z;

    let u_base = ch * height * rank;
    let v_base = ch * rank * width;

    var acc = 0.0;
    var i: u32 = 0u;
    loop {
        if (i >= rank) {
            break;
        }

        let u_idx = u_base + y * rank + i;
        let v_idx = v_base + i * width + x;
        acc += u_buf[u_idx] * v_buf[v_idx];

        i += 1u;
    }

    return acc;
}

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let width = params.size.x;
    let height = params.size.y;
    if (gid.x >= width || gid.y >= height) {
        return;
    }

    let out_idx = gid.y * width + gid.x;
    out_buf[out_idx] = vec4<f32>(
        channel_value(0u, gid.x, gid.y),
        channel_value(1u, gid.x, gid.y),
        channel_value(2u, gid.x, gid.y),
        channel_value(3u, gid.x, gid.y)
    );
}
