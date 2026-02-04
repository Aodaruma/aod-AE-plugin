let features = variable::get("features").to_string();
let has_wgpu = features.contains("wgpu");

if !has_wgpu {
    fs::remove_dir_all("src/gpu");
}
