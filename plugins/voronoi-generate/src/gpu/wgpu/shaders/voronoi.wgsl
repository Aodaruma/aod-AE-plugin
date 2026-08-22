struct Params {
    size: vec4<u32>,
    seed: vec4<u32>,
    cell: vec4<f32>,
    extra: vec4<f32>,
    misc: vec4<f32>,
};

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read_write> out_buf: array<vec4<f32>>;

struct Site {
    x: f32,
    y: f32,
    w: f32,
    hash: u32,
};

struct SmoothF1Result {
    dist: f32,
    color: vec3<f32>,
    pos: vec3<f32>,
};

fn hash_u32(mut x: u32) -> u32 {
    x = x ^ (x >> 16u);
    x = x * 0x7FEB352Du;
    x = x ^ (x >> 15u);
    x = x * 0x846CA68Bu;
    x = x ^ (x >> 16u);
    return x;
}

fn hash3(x: i32, y: i32, w: i32, seed: u32) -> u32 {
    let ux = bitcast<u32>(x);
    let uy = bitcast<u32>(y);
    let uw = bitcast<u32>(w);
    var h = seed ^ 0x9E3779B9u;
    h = h + ux * 0x85EBCA6Bu;
    h = h + uy * 0xC2B2AE35u;
    h = h + uw * 0x27D4EB2Du;
    return hash_u32(h);
}

fn rand01(h: u32) -> f32 {
    return f32(h) / 4294967295.0;
}

fn cell_point(cell_x: i32, cell_y: i32, cell_w: i32, randomness: f32, seed: u32) -> Site {
    let h = hash3(cell_x, cell_y, cell_w, seed);
    let rx = rand01(hash_u32(h ^ 0xA511E9B3u));
    let ry = rand01(hash_u32(h ^ 0x63D83595u));
    let rw = rand01(hash_u32(h ^ 0x1F1D8E33u));
    let ox = 0.5 + (rx - 0.5) * randomness;
    let oy = 0.5 + (ry - 0.5) * randomness;
    let ow = 0.5 + (rw - 0.5) * randomness;
    return Site(f32(cell_x) + ox, f32(cell_y) + oy, f32(cell_w) + ow, h);
}

fn hash_color(h: u32) -> vec3<f32> {
    let r = rand01(hash_u32(h ^ 0xB5297A4Du));
    let g = rand01(hash_u32(h ^ 0x68E31DA4u));
    let b = rand01(hash_u32(h ^ 0x1B56C4E9u));
    return vec3<f32>(r, g, b);
}

fn lerp3(a: vec3<f32>, b: vec3<f32>, t: f32) -> vec3<f32> {
    return a + (b - a) * t;
}

fn metric_distance(dx: f32, dy: f32, dw: f32, metric: u32, lp_exp: f32) -> f32 {
    let adx = abs(dx);
    let ady = abs(dy);
    let adw = abs(dw);
    if metric == 0u {
        return sqrt(dx * dx + dy * dy + dw * dw);
    }
    if metric == 1u {
        return adx + ady + adw;
    }
    if metric == 2u {
        return max(max(adx, ady), adw);
    }
    let p = max(lp_exp, 0.1);
    let s = pow(adx, p) + pow(ady, p) + pow(adw, p);
    return pow(s, 1.0 / p);
}

fn smoothstep01(x: f32) -> f32 {
    let t = clamp(x, 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

fn smooth_f1_all(
    px: f32,
    py: f32,
    pw: f32,
    cell_x: i32,
    cell_y: i32,
    cell_w: i32,
    randomness: f32,
    seed: u32,
    metric: u32,
    lp_exp: f32,
    smoothness: f32
) -> SmoothF1Result {
    let k = max(smoothness, 1e-20); // division用
    var sd: f32 = 0.0;
    var first: bool = true;
    var scol: vec3<f32> = vec3<f32>(0.0, 0.0, 0.0);
    var spos: vec3<f32> = vec3<f32>(0.0, 0.0, 0.0);

    for (var nw: i32 = cell_w - 2; nw <= cell_w + 2; nw = nw + 1) {
        for (var ny: i32 = cell_y - 2; ny <= cell_y + 2; ny = ny + 1) {
            for (var nx: i32 = cell_x - 2; nx <= cell_x + 2; nx = nx + 1) {
                let site = cell_point(nx, ny, nw, randomness, seed);
                let d = metric_distance(px - site.x, py - site.y, pw - site.w, metric, lp_exp);
                if (first) {
                    sd = d;
                    first = false;
                    scol = hash_color(site.hash);
                    spos = vec3<f32>(site.x, site.y, site.w);
                    continue;
                }
                let x = clamp(0.5 + 0.5 * (sd - d) / k, 0.0, 1.0);
                let h = smoothstep01(x);
                let corr_d = smoothness * h * (1.0 - h);
                sd = lerp(sd, d, h) - corr_d;

                // Blender実装と同様に、color/positionは補正を弱める
                let corr_attr = corr_d / (1.0 + 3.0 * smoothness);
                let cell_col = hash_color(site.hash);
                let site_pos = vec3<f32>(site.x, site.y, site.w);
                scol = lerp3(scol, cell_col, h) - vec3<f32>(corr_attr);
                spos = lerp3(spos, site_pos, h) - vec3<f32>(corr_attr);
            }
        }
    }
    let safe_sd = select(0.0, sd, isFinite(sd));
    return SmoothF1Result(safe_sd, scol, spos);
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    return a + (b - a) * t;
}

fn n_sphere_radius(nearest: Site, randomness: f32, seed: u32) -> f32 {
    // Blender定義: closest feature point と「その点に最も近い別feature point」の距離 / 2
    // ここは distance(metric) ではなく Euclidean 距離に寄せる
    let ncx = i32(floor(nearest.x));
    let ncy = i32(floor(nearest.y));
    let ncw = i32(floor(nearest.w));
    var min_d = 1e30;

    for (var dw: i32 = -1; dw <= 1; dw = dw + 1) {
        for (var dy: i32 = -1; dy <= 1; dy = dy + 1) {
            for (var dx: i32 = -1; dx <= 1; dx = dx + 1) {
                if (dx == 0 && dy == 0 && dw == 0) {
                    continue;
                }
                let s = cell_point(ncx + dx, ncy + dy, ncw + dw, randomness, seed);
                let vx = nearest.x - s.x;
                let vy = nearest.y - s.y;
                let vw = nearest.w - s.w;
                let d = sqrt(vx * vx + vy * vy + vw * vw);
                min_d = select(min_d, d, d < min_d);
            }
        }
    }
    return select(0.0, 0.5 * min_d, isFinite(min_d));
}

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let out_w = params.size.x;
    let out_h = params.size.y;
    if (gid.x >= out_w || gid.y >= out_h) {
        return;
    }

    let inv_cell_x = params.cell.x;
    let inv_cell_y = params.cell.y;
    let randomness = params.cell.z;
    let lp_exp = params.cell.w;
    let inv_cell_w = params.extra.x;
    let use_local = params.extra.y;
    let origin_x = params.extra.z;
    let origin_y = params.extra.w;
    let smoothness = params.misc.x;
    let w_value = params.misc.y;
    let offset_x = params.misc.z;
    let offset_y = params.misc.w;
    let feature_mode = params.seed.y;

    let base_x = f32(gid.x) + 0.5;
    let base_y = f32(gid.y) + 0.5;
    let sample_x = select(base_x + origin_x, base_x, use_local > 0.5);
    let sample_y = select(base_y + origin_y, base_y, use_local > 0.5);

    let px = (sample_x - offset_x) * inv_cell_x;
    let py = (sample_y - offset_y) * inv_cell_y;
    let pw = w_value * inv_cell_w;
    let cell_x = i32(floor(px));
    let cell_y = i32(floor(py));
    let cell_w = i32(floor(pw));

    var d1 = 1e20;
    var d2 = 1e20;
    var nearest = Site(0.0, 0.0, 0.0, 0u);
    var second = Site(0.0, 0.0, 0.0, 0u);

    for (var nw: i32 = cell_w - 1; nw <= cell_w + 1; nw = nw + 1) {
        for (var ny: i32 = cell_y - 1; ny <= cell_y + 1; ny = ny + 1) {
            for (var nx: i32 = cell_x - 1; nx <= cell_x + 1; nx = nx + 1) {
                let site = cell_point(nx, ny, nw, randomness, params.seed.x);
                let dx = px - site.x;
                let dy = py - site.y;
                let dw = pw - site.w;
                let d = metric_distance(dx, dy, dw, params.size.z, lp_exp);
                if (d < d1) {
                    d2 = d1;
                    second = nearest;
                    d1 = d;
                    nearest = site;
                } else if (d < d2) {
                    d2 = d;
                    second = site;
                }
            }
        }
    }

    if (d2 < d1) {
        let tmp = d1;
        d1 = d2;
        d2 = tmp;
        let tmp_site = nearest;
        nearest = second;
        second = tmp_site;
    }

    var out = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    if (params.size.w == 0u) {
        // Color:
        // - F1 / Smooth(F1): feature point color（Smoothならブレンド）
        // - F2: 2nd feature point color
        // - その他（F2-F1, N-sphere等）: Blender互換寄せでモノクロ（distance値）
        if (feature_mode == 0u && smoothness > 0.0) {
            let s = smooth_f1_all(
                px, py, pw,
                cell_x, cell_y, cell_w,
                randomness, params.seed.x,
                params.size.z, lp_exp,
                smoothness
            );
            out = vec4<f32>(s.color, 1.0);
        } else if (feature_mode == 1u) {
            out = vec4<f32>(hash_color(second.hash), 1.0);
        } else if (feature_mode == 2u) {
            let v = max(d2 - d1, 0.0);
            out = vec4<f32>(v, v, v, 1.0);
        } else {
            let v = n_sphere_radius(nearest, randomness, params.seed.x);
            out = vec4<f32>(v, v, v, 1.0);
        }
    } else if (params.size.w == 1u) {
        // Position:
        // - F1 / Smooth(F1): feature point position（Smoothならブレンド）
        // - F2: 2nd feature point position
        var p = vec3<f32>(nearest.x, nearest.y, nearest.w);
        if (feature_mode == 0u && smoothness > 0.0) {
            let s = smooth_f1_all(
                px, py, pw,
                cell_x, cell_y, cell_w,
                randomness, params.seed.x,
                params.size.z, lp_exp,
                smoothness
            );
            p = s.pos;
        } else if (feature_mode == 1u) {
            p = vec3<f32>(second.x, second.y, second.w);
        }

        if (use_local > 0.5) {
            let site_world_x = p.x / inv_cell_x + offset_x;
            let site_world_y = p.y / inv_cell_y + offset_y;
            let r = site_world_x / f32(out_w);
            let g = site_world_y / f32(out_h);
            out = vec4<f32>(r, g, 0.0, 1.0);
        } else {
            let grid_w = max(f32(out_w) * inv_cell_x, 1e-6);
            let grid_h = max(f32(out_h) * inv_cell_y, 1e-6);
            let r = p.x / grid_w;
            let g = p.y / grid_h;
            out = vec4<f32>(r, g, 0.0, 1.0);
        }
    } else if (params.size.w == 2u) {
        var v: f32;
        if (feature_mode == 0u) {
            v = d1;
            if (smoothness > 0.0) {
                v = smooth_f1_all(
                    px, py, pw,
                    cell_x, cell_y, cell_w,
                    randomness, params.seed.x,
                    params.size.z, lp_exp,
                    smoothness
                ).dist;
            }
        } else if (feature_mode == 1u) {
            v = d2;
        } else if (feature_mode == 2u) {
            v = max(d2 - d1, 0.0);
        } else {
            v = n_sphere_radius(nearest, randomness, params.seed.x);
        }
        out = vec4<f32>(v, v, v, 1.0);
    }

    let idx = gid.y * out_w + gid.x;
    out_buf[idx] = out;
}
