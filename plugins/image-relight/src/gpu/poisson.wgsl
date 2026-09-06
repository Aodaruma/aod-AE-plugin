const BACKGROUND_LABEL: u32 = 0xffffffffu;
const DIRICHLET: u32 = 0u;

struct Params {
    image: vec4<u32>,
    coefficients: vec4<f32>,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> labels: array<u32>;
@group(0) @binding(2) var<storage, read> boundary: array<u32>;
@group(0) @binding(3) var<storage, read> rhs: array<f32>;
@group(0) @binding(4) var<storage, read_write> heights: array<f32>;

fn relax(coords: vec2<u32>, parity: u32) {
    let width = params.image.x;
    let height = params.image.y;
    if coords.x >= width || coords.y >= height {
        return;
    }
    if ((coords.x ^ coords.y) & 1u) != parity {
        return;
    }

    let index = coords.y * width + coords.x;
    let label = labels[index];
    if label == BACKGROUND_LABEL {
        return;
    }
    let dirichlet = params.image.z == DIRICHLET;
    if dirichlet && boundary[index] != 0u {
        return;
    }

    let lambda_squared = params.coefficients.x;
    let weight_x = params.coefficients.y;
    let weight_y = params.coefficients.z;
    let omega = params.coefficients.w;
    var sum = 0.0;
    var diagonal = lambda_squared;

    if coords.x > 0u {
        let neighbor = index - 1u;
        if labels[neighbor] == label {
            sum += weight_x * heights[neighbor];
            diagonal += weight_x;
        } else if dirichlet {
            diagonal += weight_x;
        }
    } else if dirichlet {
        diagonal += weight_x;
    }

    if coords.x + 1u < width {
        let neighbor = index + 1u;
        if labels[neighbor] == label {
            sum += weight_x * heights[neighbor];
            diagonal += weight_x;
        } else if dirichlet {
            diagonal += weight_x;
        }
    } else if dirichlet {
        diagonal += weight_x;
    }

    if coords.y > 0u {
        let neighbor = index - width;
        if labels[neighbor] == label {
            sum += weight_y * heights[neighbor];
            diagonal += weight_y;
        } else if dirichlet {
            diagonal += weight_y;
        }
    } else if dirichlet {
        diagonal += weight_y;
    }

    if coords.y + 1u < height {
        let neighbor = index + width;
        if labels[neighbor] == label {
            sum += weight_y * heights[neighbor];
            diagonal += weight_y;
        } else if dirichlet {
            diagonal += weight_y;
        }
    } else if dirichlet {
        diagonal += weight_y;
    }

    var candidate = 0.0;
    if diagonal > 1.0e-12 {
        candidate = (rhs[index] + sum) / diagonal;
    }
    let old = heights[index];
    let updated = old + omega * (candidate - old);
    if updated == updated && abs(updated) <= 3.4028234663852886e+38 {
        heights[index] = updated;
    }
}

@compute @workgroup_size(16, 16, 1)
fn red(@builtin(global_invocation_id) global_id: vec3<u32>) {
    relax(global_id.xy, 0u);
}

@compute @workgroup_size(16, 16, 1)
fn black(@builtin(global_invocation_id) global_id: vec3<u32>) {
    relax(global_id.xy, 1u);
}
