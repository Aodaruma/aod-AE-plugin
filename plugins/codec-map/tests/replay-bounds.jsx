// SPDX-License-Identifier: MPL-2.0
// Run in an empty AE project after smoke.ps1 has generated the input fixtures.
(function () {
    $.evalFile(new File(File($.fileName).parent.fsName + "/controls.jsxinc"));
    var root = new Folder(File($.fileName).parent.parent.parent.parent.fsName + "/target/codec-map-replay-bounds");
    root.create();
    var log = new File(root.fsName + "/host-report.txt");
    function note(s) { log.open("a"); log.writeln(s); log.close(); }
    function save(comp, name, frame) {
        comp.time = frame / 24;
        comp.saveFrameToPng(comp.time, new File(root.fsName + "/" + name + ".png"));
        note("requested " + name);
    }
    try {
        if (app.project.numItems !== 0) { throw new Error("Bounds test needs an empty project"); }
        log.open("w"); log.close();
        app.project.bitsPerChannel = 8;
        var source = app.project.importFile(new ImportOptions(new File(root.parent.fsName + "/codec-map-smoke/source.png")));
        var comp = app.project.items.addComp("Animated input bounds", 400, 300, 1, 2, 24);
        var layer = comp.layers.add(source);
        var effects = layer.property("ADBE Effect Parade");
        effects.addProperty("ADBE GROW BOUNDS");
        var fx = effects.addProperty("CodecMap");
        var grow = effects.property(1).property(1);
        codecParam(fx, "Quality Control").setValue(2);
        if (codecParam(fx, "Replay Memory (MiB)").value !== 1024) {
            throw new Error("Replay Memory ID/default mismatch");
        }
        if (codecParam(fx, "Replay Memory (MiB)").canVaryOverTime) { throw new Error("Memory setting must be fixed"); }
        codecParam(fx, "Output").setValue(2);
        codecParam(fx, "GOP Length").setValue(1);
        note("AE " + app.version + "; memory default=" + codecParam(fx, "Replay Memory (MiB)").value);
        for (var codec = 1; codec <= 3; codec++) {
            codecParam(fx, "Codec").setValue(codec);
            for (var direction = 0; direction < 2; direction++) {
                // Grow Bounds changes both dimensions and buffer origin. The
                // actual image remains stationary in layer coordinates.
                grow.expression = direction === 0 ? "2 + time*24*4" : "42 - time*24*4";
                var prefix = "codec" + codec + (direction === 0 ? "-grow" : "-shrink");
                codecParam(fx, "Drop I-Frames").setValue(1);
                save(comp, prefix + "-drop", 5);
                app.purge(PurgeTarget.ALL_CACHES);
                save(comp, prefix + "-other", 2);
                save(comp, prefix + "-repeat", 5);
                codecParam(fx, "Drop I-Frames").setValue(0);
                save(comp, prefix + "-reference", 5);
            }
        }
        // Ninety 1920x1080 YUV frames exceed the old 256 MiB limit.
        var large = app.project.items.addComp("Replay above 256 MiB", 1920, 1080, 1, 300, 24);
        var solid = large.layers.addSolid([0.2, 0.6, 0.3], "Memory probe", 1920, 1080, 1, 300);
        var memory = solid.property("ADBE Effect Parade").addProperty("CodecMap");
        codecParam(memory, "Quality Control").setValue(2);
        codecParam(memory, "Codec").setValue(3);
        codecParam(memory, "GOP Length").setValue(1);
        codecParam(memory, "Output").setValue(2);
        save(large, "memory-seed", 0);
        codecParam(memory, "Drop I-Frames").setValue(1);
        save(large, "memory-270-mib", 89);
        app.purge(PurgeTarget.ALL_CACHES);
        save(large, "memory-270-mib-repeat", 89);
        // A low budget and >6000 frames must not block non-codec previews.
        codecParam(memory, "Replay Memory (MiB)").setValue(64);
        codecParam(memory, "Mix").setValue(0);
        save(large, "bypass-mix-zero", 6500);
        memory.enabled = false;
        save(large, "bypass-reference", 6500);
        memory.enabled = true;
        codecParam(memory, "Mix").setValue(100);
        codecParam(memory, "Preview").setValue(2);
        save(large, "bypass-map", 6500);
        codecParam(memory, "Preview").setValue(3);
        save(large, "bypass-offset", 6500);
        codecParam(memory, "Preview").setValue(1);
        codecParam(memory, "Replay Memory (MiB)").setValue(1024);
        large.time = 89 / 24;
        app.project.save(new File(root.fsName + "/replay-bounds.aep"));
        note("Completed. Validate PNG files and pixel comparisons externally.");
    } catch (e) { note("FAIL " + e.toString() + " line " + e.line); throw e; }
}());
