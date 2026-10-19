// Run after ae_smoke_test.jsx in the authorized test project.
(function () {
    var root = $.global.AOD_SEAM_TEST_ROOT ? new Folder($.global.AOD_SEAM_TEST_ROOT) : File($.fileName).parent.parent.parent.parent;
    if (!new File(root.fsName + "/plugins/seam-scale/Cargo.toml").exists) {
        throw new Error("Set AOD_SEAM_TEST_ROOT to the worktree root when running through MCP.");
    }
    var out = new Folder(root.fsName + "/target/seam-scale-ae-0.1.5");
    var log = new File(out.fsName + "/benchmark.txt");
    if (!log.open("w")) { throw new Error("Cannot open benchmark output log."); }
    function record(text) { log.writeln(text); log.close(); log.open("a"); }
    app.beginSuppressDialogs();
    try {
        if (!app.project.file || app.project.file.name !== "seam-scale-smoke.aep") {
            throw new Error("Open the SeamScale smoke test project first.");
        }
        if (app.project.renderQueue.numItems !== 0) { throw new Error("Expected an empty test render queue."); }
        app.project.bitsPerChannel = 8;
        var source = app.project.items.addComp("Benchmark Source 1080p", 1920, 1080, 1, 1, 24);
        var bg = source.layers.addSolid([0.05,0.08,0.15], "Gradient", 1920, 1080, 1);
        var ramp = bg.property("ADBE Effect Parade").addProperty("ADBE Ramp");
        ramp.property(1).setValue([0,0]);
        ramp.property(2).setValue([0.03,0.07,0.12,1]);
        ramp.property(3).setValue([1920,1080]);
        ramp.property(4).setValue([0.42,0.68,0.85,1]);
        for (var i = 0; i < 12; i++) {
            var strip = source.layers.addSolid([0.15+i*0.06,0.8-i*0.045,0.22+i*0.04], "Animated strip "+i, 46+i*4, 270+i*31, 1);
            strip.position.setValueAtTime(0,[100+i*148,350+i*23]);
            strip.position.setValueAtTime(1,[155+i*130,530-i*15]);
        }
        var comp = app.project.items.addComp("SeamScale 1080p 50 percent",1920,1080,1,1,24);
        var layer = comp.layers.add(source);
        var effect = layer.property("ADBE Effect Parade").addProperty("SeamScale");
        effect.property("Scale (%)").setValue(50);
        record("After Effects " + app.version + "; 1920x1080; 8bpc; RGBA; shared 50%; restore OFF; animated synthetic input");
        record("Time includes source rendering, SeamScale and PNG encoding/writing. Three distinct times; not original user footage.");
        // saveFrameToPng queues work until the script returns, so it cannot be timed here.
        var item = app.project.renderQueue.items.add(comp);
        item.applyTemplate("Best Settings");
        item.timeSpanStart = 0;
        item.timeSpanDuration = 3/24;
        var module = item.outputModule(1);
        record("Output templates: " + module.templates.join(" | "));
        var pngTemplate = null;
        for (var t = 0; t < module.templates.length; t++) {
            if (/^PNG/i.test(module.templates[t])) { pngTemplate = module.templates[t]; break; }
        }
        if (!pngTemplate) { throw new Error("A PNG output module template is required."); }
        module.applyTemplate(pngTemplate);
        record("Selected output template: " + pngTemplate);
        record("Output settings: " + module.getSettings(GetSettingsFormat.STRING).toSource());
        module.file = new File(out.fsName+"/benchmark_[#####].png");
        var start = new Date().getTime();
        app.project.renderQueue.render();
        var elapsed = new Date().getTime()-start;
        if (item.status !== RQItemStatus.DONE) { throw new Error("Render queue did not finish."); }
        record("frames=3 total_ms="+elapsed+" average_ms="+(elapsed/3));
        app.project.save(new File(out.fsName+"/seam-scale-benchmark.aep"));
        comp.openInViewer();
        layer.selected = true;
        record("PASS benchmark renders completed");
    } catch(error) { record("FAIL "+error.toString()+" line="+error.line); }
    finally { log.close(); app.endSuppressDialogs(false); }
}());
