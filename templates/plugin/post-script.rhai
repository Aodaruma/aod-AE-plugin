let features = variable::get("features").to_string();
let has_wgpu = features.contains("wgpu");

if !has_wgpu {
    file::delete("src/gpu");
}
