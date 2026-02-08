struct Params {
    size: vec4<u32>, // x=width, y=height, z=axis, w=edge_mode
    mode: vec4<u32>, // x=out_mode, y=raw32, z=rgb_only
    map: vec4<f32>,  // x=offset, y=scale
};

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> in_buf: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> out_buf: array<vec4<f32>>;

fn rem_euclid_f(x: f32, y: f32) -> f32 {
    return x - y * floor(x / y);
}

fn wrap_index(i: i32, len: i32) -> i32 {
    if (len <= 0) {
        return 0;
    }
    return i32(rem_euclid_f(f32(i), f32(len)));
}

fn mirror_index(i: i32, len: i32) -> i32 {
    if (len <= 1) {
        return 0;
    }
    let period = len * 2 - 2;
    let t = i32(rem_euclid_f(f32(i), f32(period)));
    if (t < len) {
        return t;
    }
    return period - t;
}

fn resolve_coord(coord: i32, len: i32, edge_mode: u32) -> i32 {
    // 0=None, 1=Repeat, 2=Tile, 3=Mirror
    if (edge_mode == 0u) {
        if (coord < 0 || coord >= len) {
            return -1;
        }
        return coord;
    }
    if (edge_mode == 1u) {
        return clamp(coord, 0, len - 1);
    }
    if (edge_mode == 2u) {
        return wrap_index(coord, len);
    }
    return mirror_index(coord, len);
}

fn sample_pixel(ix: i32, iy: i32) -> vec4<f32> {
    let w = i32(params.size.x);
    let h = i32(params.size.y);
    let rx = resolve_coord(ix, w, params.size.w);
    let ry = resolve_coord(iy, h, params.size.w);
    if (rx < 0 || ry < 0) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }
    let idx = u32(ry) * params.size.x + u32(rx);
    return in_buf[idx];
}

fn soft_clamp01(v: f32) -> f32 {
    let centered = v - 0.5;
    return 0.5 + 0.5 * (centered / (1.0 + abs(centered)));
}

fn mirror01(v: f32) -> f32 {
    let t = rem_euclid_f(v, 2.0);
    if (t <= 1.0) {
        return t;
    }
    return 2.0 - t;
}

fn wrap01(v: f32) -> f32 {
    return rem_euclid_f(v, 1.0);
}

fn map_value(diff: f32) -> f32 {
    let mapped_base = params.map.x;
    let raw_base = params.map.x - 0.5;
    let base = select(mapped_base, raw_base, params.mode.y != 0u);

    var v = base + diff * params.map.y;
    if (!isFinite(v)) {
        v = 0.0;
    }

    // 0=Clamp, 1=SoftClamp, 2=Mirror, 3=Wrap, 4=None
    if (params.mode.x == 0u) {
        return clamp(v, 0.0, 1.0);
    }
    if (params.mode.x == 1u) {
        return soft_clamp01(v);
    }
    if (params.mode.x == 2u) {
        return mirror01(v);
    }
    if (params.mode.x == 3u) {
        return wrap01(v);
    }
    return v;
}

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let out_w = params.size.x;
    let out_h = params.size.y;
    if (gid.x >= out_w || gid.y >= out_h) {
        return;
    }

    let x = i32(gid.x);
    let y = i32(gid.y);
    let center = sample_pixel(x, y);
    let left = sample_pixel(x - 1, y);
    let right = sample_pixel(x + 1, y);
    let up = sample_pixel(x, y - 1);
    let down = sample_pixel(x, y + 1);

    let dx = (right - left) * 0.5;
    let dy = (down - up) * 0.5;

    var diff = dx;
    if (params.size.z == 1u) {
        diff = dy;
    } else if (params.size.z == 2u) {
        diff = sqrt(dx * dx + dy * dy);
    }

    var out_a = map_value(diff.w);
    if (params.mode.z != 0u) {
        out_a = center.w;
    }

    let out_px = vec4<f32>(
        map_value(diff.x),
        map_value(diff.y),
        map_value(diff.z),
        out_a
    );

    let idx = gid.y * out_w + gid.x;
    out_buf[idx] = out_px;
}
