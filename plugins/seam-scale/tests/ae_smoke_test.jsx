// Run only in an authorized, empty AE test project.
(function () {
    // MCP evaluates source inside its runtime; $.fileName then names the bridge.
    var root = $.global.AOD_SEAM_TEST_ROOT ? new Folder($.global.AOD_SEAM_TEST_ROOT) : File($.fileName).parent.parent.parent.parent;
    if (!new File(root.fsName + "/plugins/seam-scale/Cargo.toml").exists) {
        throw new Error("Set AOD_SEAM_TEST_ROOT to the worktree root when running through MCP.");
    }
    var out = new Folder(root.fsName + "/target/seam-scale-ae-0.2.0");
    if (!out.exists && !out.create()) { throw new Error("Cannot create test output directory."); }
    var log = new File(out.fsName + "/result.txt");
    if (!log.open("w")) { throw new Error("Cannot open test output log."); }
    function record(message) { log.writeln(message); log.close(); log.open("a"); }
    app.beginSuppressDialogs();
    try {
        record("After Effects " + app.version);
        if (app.project.numItems !== 0) { throw new Error("Use a new, empty test instance."); }
        function comp(name, w, h) { return app.project.items.addComp(name, w || 160, h || 120, 1, 1, 24); }
        function effect(layer) { return layer.property("ADBE Effect Parade").addProperty("SeamScale"); }
        function save(c, name) {
            c.saveFrameToPng(0, new File(out.fsName + "/" + name + ".png"));
            record("rendered " + name);
        }
        var source = comp("Source");
        source.layers.addSolid([0.04, 0.09, 0.14], "Background", 160, 120, 1);
        for (var i = 0; i < 5; i++) {
            var strip = source.layers.addSolid([0.18 + i * 0.13, 0.75 - i * 0.1, 0.2 + i * 0.12], "Strip " + i, 14, 76 - i * 7, 1);
            strip.position.setValue([28 + i * 25, 35 + i * 10]);
        }
        for (var bi = 0; bi < 3; bi++) {
            var depth = [8, 16, 32][bi];
            app.project.bitsPerChannel = depth;
            var c = comp("Pattern " + depth);
            var layer = c.layers.add(source);
            save(c, "input-" + depth);
            var e = effect(layer);
            if (e.property("Output").value !== 1) { throw new Error("Expected default Image output"); }
            if (e.property("Seam Channel").value !== 2) { throw new Error("Expected default RGBA channel"); }
            save(c, "identity-" + depth);
            e.property("Scale (%)").setValue(75);
            save(c, "linked-" + depth);
            e.property("Separate X/Y Scale").setValue(1);
            e.property("Scale X (%)").setValue(75);
            e.property("Scale Y (%)").setValue(75);
            save(c, "separate-equal-" + depth);
            e.property("Scale Y (%)").setValue(100);
            save(c, "x-only-" + depth);
            e.property("Restore Original Size").setValue(1);
            save(c, "restored-" + depth);
            e.property("Scale X (%)").setValue(140);
            e.property("Scale Y (%)").setValue(80);
            save(c, "mixed-" + depth);
            // Nested crop exercises the Smart Render output buffer origin.
            var cropped = comp("Crop " + depth, 80, 60);
            cropped.layers.add(c).position.setValue([40, 30]);
            save(cropped, "crop-" + depth);
            for (var channel = 1; channel <= 7; channel++) {
                e.property("Seam Channel").setValue(channel);
                e.property("Restore Original Size").setValue(0);
                e.property("Scale X (%)").setValue(50);
                e.property("Scale Y (%)").setValue(75);
                save(c, "channel-" + channel + "-positive-" + depth);
                e.property("Scale X (%)").setValue(-50);
                save(c, "channel-" + channel + "-flip-x-" + depth);
                e.property("Scale Y (%)").setValue(-75);
                save(c, "channel-" + channel + "-flip-xy-" + depth);
                e.property("Scale X (%)").setValue(50);
                save(c, "channel-" + channel + "-flip-y-" + depth);
                e.property("Scale X (%)").setValue(150);
                e.property("Scale Y (%)").setValue(125);
                e.property("Restore Original Size").setValue(1);
                save(c, "channel-" + channel + "-expand-restored-" + depth);
            }
            e.property("Seam Channel").setValue(2);
            e.property("Separate X/Y Scale").setValue(0);
            e.property("Scale (%)").setValue(-50);
            save(c, "shared-negative-" + depth);
            e.property("Separate X/Y Scale").setValue(1);
            e.property("Scale X (%)").setValue(-50);
            e.property("Scale Y (%)").setValue(-50);
            save(c, "separate-negative-" + depth);
            e.property("Scale X (%)").setValue(0);
            e.property("Scale Y (%)").setValue(100);
            e.property("Restore Original Size").setValue(0);
            save(c, "zero-x-" + depth);
            e.property("Scale X (%)").setValue(140);
            e.property("Scale Y (%)").setValue(80);
            e.property("Restore Original Size").setValue(1);
            // Alpha-only input bounds must not become the carving canvas.
            var masked = comp("Masked " + depth);
            var solid = masked.layers.addSolid([0.8, 0.3, 0.1], "Masked Solid", 160, 120, 1);
            var mask = solid.Masks.addProperty("ADBE Mask Atom");
            var shape = new Shape();
            shape.vertices = [[30, 25], [125, 25], [125, 90], [30, 90]];
            shape.inTangents = [[0,0],[0,0],[0,0],[0,0]];
            shape.outTangents = [[0,0],[0,0],[0,0],[0,0]];
            shape.closed = true;
            mask.maskShape.setValue(shape);
            var full = comp("Precomposed Mask " + depth);
            var pre = full.layers.add(masked);
            var pe = effect(pre);
            pe.property("Scale (%)").setValue(75);
            save(full, "mask-precomp-" + depth);
            var me = effect(solid);
            me.property("Scale (%)").setValue(75);
            save(masked, "mask-direct-" + depth);
            var flat = comp("Flat " + depth);
            var fl = flat.layers.addSolid([0.2, 0.6, 0.9], "Flat", 160, 120, 1);
            var fe = effect(fl);
            fe.property("Separate X/Y Scale").setValue(1);
            fe.property("Scale X (%)").setValue(50);
            fe.property("Scale Y (%)").setValue(75);
            save(flat, "flat-shrink-" + depth);
            fe.property("Restore Original Size").setValue(1);
            save(flat, "flat-restore-" + depth);
            fe.property("Scale X (%)").setValue(400);
            fe.property("Scale Y (%)").setValue(400);
            save(flat, "flat-expand-" + depth);
            c.resolutionFactor = [2, 2];
            save(c, "half-" + depth);
        }
        app.project.bitsPerChannel = 8;
        app.project.save(new File(out.fsName + "/seam-scale-smoke.aep"));
        record("PASS all AE render calls completed");
    } catch (error) {
        record("FAIL " + error.toString() + " line=" + error.line);
    } finally {
        log.close();
        app.endSuppressDialogs(false);
    }
}());
