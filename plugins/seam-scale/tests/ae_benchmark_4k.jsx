// Run only in the saved SeamScale test project. Set the root and version label first.
(function () {
    var root = new Folder($.global.AOD_SEAM_TEST_ROOT || "");
    var label = $.global.AOD_SEAM_BENCH_LABEL;
    var w = $.global.AOD_SEAM_BENCH_WIDTH || 3840;
    var h = w * 9 / 16;
    if (!new File(root.fsName + "/plugins/seam-scale/Cargo.toml").exists ||
        !/^v0\.1\.[0-9]+-(4k|1080p)$/.test(label || "") ||
        (w !== 3840 && w !== 1920)) { throw new Error("Invalid test configuration."); }
    if (!app.project.file || !/^seam-scale-benchmark(?:-4k)?\.aep$/.test(app.project.file.name) ||
        app.project.file.fsName.toLowerCase().indexOf((root.fsName + "/target/").replace(/\//g, "\\").toLowerCase()) !== 0) {
        throw new Error("Open the saved SeamScale benchmark project inside this worktree.");
    }
    var parent = new Folder(root.fsName + "/target/seam-scale-ae-0.1.5");
    if (!parent.exists && !parent.create()) { throw new Error("Cannot create output root."); }
    var out = new Folder(parent.fsName + "/" + label);
    if (!out.exists && !out.create()) { throw new Error("Cannot create output folder."); }
    var log = new File(out.fsName + "/benchmark.txt");
    if (!log.open("w")) { throw new Error("Cannot open output log."); }
    function record(value) { log.writeln(value); log.close(); log.open("a"); }
    app.beginSuppressDialogs();
    try {
        var name = "SeamScale " + w + "x" + h + " 50 percent";
        var comp = null;
        for (var i = 1; i <= app.project.numItems; i++) {
            if (app.project.item(i) instanceof CompItem && app.project.item(i).name === name) {
                comp = app.project.item(i); break;
            }
        }
        if (!comp) {
            var factor = w / 1920;
            var source = app.project.items.addComp("Benchmark Source " + w + "x" + h, w, h, 1, 1, 24);
            var bg = source.layers.addSolid([0.05,0.08,0.15], "Gradient", w, h, 1);
            var ramp = bg.property("ADBE Effect Parade").addProperty("ADBE Ramp");
            ramp.property(1).setValue([0,0]);
            ramp.property(2).setValue([0.03,0.07,0.12,1]);
            ramp.property(3).setValue([w,h]);
            ramp.property(4).setValue([0.42,0.68,0.85,1]);
            for (var j = 0; j < 12; j++) {
                var strip = source.layers.addSolid([0.15+j*0.06,0.8-j*0.045,0.22+j*0.04], "Animated strip " + j, (46+j*4)*factor, (270+j*31)*factor, 1);
                strip.position.setValueAtTime(0,[(100+j*148)*factor,(350+j*23)*factor]);
                strip.position.setValueAtTime(1,[(155+j*130)*factor,(530-j*15)*factor]);
            }
            comp = app.project.items.addComp(name,w,h,1,1,24);
            var layer = comp.layers.add(source);
            layer.property("ADBE Effect Parade").addProperty("SeamScale").property("Scale (%)").setValue(50);
        }
        app.project.bitsPerChannel = 8;
        comp.resolutionFactor = [1,1];
        var queue = app.project.renderQueue;
        var item = null;
        for (var q = 1; q <= queue.numItems; q++) {
            var existing = queue.item(q);
            if (existing.status === RQItemStatus.QUEUED) { existing.render = false; }
            if (existing.comp.id === comp.id) { item = existing; }
        }
        if (!item) { item = queue.items.add(comp); }
        item.applyTemplate("Best Settings");
        item.render = true;
        item.timeSpanStart = 0;
        item.timeSpanDuration = 3/24;
        var module = item.outputModule(1);
        module.applyTemplate("PNG seq (untone-map)");
        module = item.outputModule(1);
        module.file = new File(out.fsName + "/benchmark_[#####].png");
        record("AE " + app.version + "; " + label + "; " + w + "x" + h + "; shared 50%; RGBA; restore OFF; 8bpc; full resolution; 3 distinct frames");
        record("Output settings: " + module.getSettings(GetSettingsFormat.STRING).toSource());
        var start = new Date().getTime();
        queue.render();
        var elapsed = new Date().getTime() - start;
        if (item.status !== RQItemStatus.DONE) { throw new Error("Render did not finish."); }
        record("frames=3 total_ms=" + elapsed + " average_ms=" + elapsed/3);
        app.project.save(new File(out.fsName + "/seam-scale-benchmark-4k.aep"));
        comp.openInViewer(); comp.time = 0; comp.layer(1).selected = true;
        record("PASS benchmark renders completed");
    } catch (error) { record("FAIL " + error.toString() + " line=" + error.line); }
    finally { log.close(); app.endSuppressDialogs(false); }
}());
