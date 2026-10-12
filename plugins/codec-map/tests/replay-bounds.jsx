// SPDX-License-Identifier: MPL-2.0
// Run in an empty AE project after smoke.ps1 has generated the input fixtures.
(function () {
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
        if (fx.property("Replay Memory (MiB)").propertyIndex !== 22 || fx.property("Replay Memory (MiB)").value !== 1024) {
            throw new Error("Replay Memory ID/default mismatch");
        }
        if (fx.property("Replay Memory (MiB)").canVaryOverTime) { throw new Error("Memory setting must be fixed"); }
        fx.property("Output").setValue(2);
        fx.property("GOP Length").setValue(1);
        note("AE " + app.version + "; memory default=" + fx.property("Replay Memory (MiB)").value);
        for (var codec = 1; codec <= 3; codec++) {
            fx.property("Codec").setValue(codec);
            for (var direction = 0; direction < 2; direction++) {
                // Grow Bounds changes both dimensions and buffer origin. The
                // actual image remains stationary in layer coordinates.
                grow.expression = direction === 0 ? "2 + time*24*4" : "42 - time*24*4";
                var prefix = "codec" + codec + (direction === 0 ? "-grow" : "-shrink");
                fx.property("Drop I-Frames").setValue(1);
                save(comp, prefix + "-drop", 5);
                app.purge(PurgeTarget.ALL_CACHES);
                save(comp, prefix + "-other", 2);
                save(comp, prefix + "-repeat", 5);
                fx.property("Drop I-Frames").setValue(0);
                save(comp, prefix + "-reference", 5);
            }
        }
        // Ninety 1920x1080 YUV frames exceed the old 256 MiB limit.
        var large = app.project.items.addComp("Replay above 256 MiB", 1920, 1080, 1, 300, 24);
        var solid = large.layers.addSolid([0.2, 0.6, 0.3], "Memory probe", 1920, 1080, 1, 300);
        var memory = solid.property("ADBE Effect Parade").addProperty("CodecMap");
        memory.property("Codec").setValue(3);
        memory.property("GOP Length").setValue(1);
        memory.property("Output").setValue(2);
        save(large, "memory-seed", 0);
        memory.property("Drop I-Frames").setValue(1);
        save(large, "memory-270-mib", 89);
        app.purge(PurgeTarget.ALL_CACHES);
        save(large, "memory-270-mib-repeat", 89);
        // A low budget and >6000 frames must not block non-codec previews.
        memory.property("Replay Memory (MiB)").setValue(64);
        memory.property("Mix").setValue(0);
        save(large, "bypass-mix-zero", 6500);
        memory.enabled = false;
        save(large, "bypass-reference", 6500);
        memory.enabled = true;
        memory.property("Mix").setValue(100);
        memory.property("Preview").setValue(2);
        save(large, "bypass-map", 6500);
        memory.property("Preview").setValue(3);
        save(large, "bypass-offset", 6500);
        memory.property("Preview").setValue(1);
        memory.property("Replay Memory (MiB)").setValue(1024);
        large.time = 89 / 24;
        app.project.save(new File(root.fsName + "/replay-bounds.aep"));
        note("Completed. Validate PNG files and pixel comparisons externally.");
    } catch (e) { note("FAIL " + e.toString() + " line " + e.line); throw e; }
}());
