(function () {
    var root = new Folder(File($.fileName).parent.parent.parent.parent.fsName);
    var output = new Folder(root.fsName + "/target/ae-image-pattern-extended");
    if (!output.exists) { output.create(); }
    var report = new File(output.fsName + "/report.txt"); report.encoding = "UTF-8"; report.open("w");
    function log(s) { report.writeln(s); report.close(); report.open("a"); }
    function set(fx, name, value) { var p = fx.property(name); if (!p) { throw new Error("Missing " + name); } p.setValue(value); }
    function save(comp, name, time) {
        var f = new File(output.fsName + "/" + name + ".png"); if (f.exists) { f.remove(); }
        comp.saveFrameToPng(time === undefined ? 2 : time, f);
        var until = new Date().getTime() + 30000;
        do { f = new File(f.fsName); if (f.exists && f.length > 0) { log("PASS " + name); return; } $.sleep(25); } while (new Date().getTime() < until);
        throw new Error("Render timeout " + name);
    }
    var created = [], depth = app.project.bitsPerChannel;
    try {
        log("AE " + app.version);
        var source = app.project.items.addComp("__IP04_Animated", 256, 192, 1, 5, 24); created.push(source);
        var solid = source.layers.addSolid([1, 1, 1], "Animated color", 256, 192, 1, 5); created.push(solid.source);
        var fill = solid.property("ADBE Effect Parade").addProperty("ADBE Fill");
        fill.property("ADBE Fill-0002").expression = "[time/5,0.1,1-time/5,1]";
        var comp = app.project.items.addComp("__IP04_Time", 256, 192, 1, 5, 24); created.push(comp);
        var layer = comp.layers.add(source), fx = layer.property("ADBE Effect Parade").addProperty("ImagePattern");
        var center = fx.property("Center").value;
        if (center[0] !== 128 || center[1] !== 96) { throw new Error("Incorrect layer center: " + center); }
        for (var p = 1; p <= fx.numProperties; p++) { if (/Use Layer Center|Overlap Priority/.test(fx.property(p).name)) { throw new Error("Removed control remains"); } }
        if (fx.property("Split Overlaps").value !== 1) { throw new Error("Split Overlaps must default on"); }
        log("PASS default Center on 256x192 layer, no obsolete center / overlap controls");
        set(fx, "Sampling", 1); set(fx, "Edge", 2);
        set(fx, "Time Delta (frames)", 12);
        set(fx, "Width (px)", 48); set(fx, "Tile Width (px)", 64); set(fx, "Tile Height (px)", 48);
        set(fx, "Iterations", 3); set(fx, "Step Scale", 1); set(fx, "Step Rotation", 0);
        set(fx, "Sectors", 4); set(fx, "Mirror Alternate Sectors", 0);
        var depths = [8,16,32];
        for (var d = 0; d < depths.length; d++) {
            app.project.bitsPerChannel = depths[d]; var suffix = "-" + depths[d];
            for (var t = 0; t <= 8; t++) { save(source, "source-" + t + suffix, t * 0.5); }
            set(fx, "Pattern Mode", 3); save(comp, "time-strip" + suffix);
            set(fx, "Time Origin (i = 0)", [128,144]); save(comp, "time-strip-origin" + suffix);
            set(fx, "Time Origin (i = 0)", [128,96]);
            set(fx, "Pattern Mode", 4);
            for (var shape = 1; shape <= 3; shape++) {
                set(fx, "Shape", shape);
                for (var axis = 1; axis <= 3; axis++) {
                    set(fx, "Tile Time Direction", axis); save(comp, "time-tile-" + shape + "-" + axis + suffix);
                }
            }
            set(fx, "Shape", 2); set(fx, "Tile Time Direction", 2);
            set(fx, "Time Origin (i = 0)", [192,96]); save(comp, "time-tile-origin" + suffix);
            set(fx, "Time Origin (i = 0)", [128,96]);
            set(fx, "Pattern Mode", 2); save(comp, "time-polar" + suffix);
            set(fx, "Pattern Mode", 1); set(fx, "Composite Order", 1); save(comp, "time-feedback-front" + suffix);
            set(fx, "Composite Order", 2); save(comp, "time-feedback-back" + suffix);
            set(fx, "Composite Order", 1); set(fx, "Time Delta (frames)", 120); save(comp, "time-feedback-held" + suffix);
            set(fx, "Time Delta (frames)", 12);
            set(fx, "Pattern Mode", 3); set(fx, "Time Delta (frames)", -12); save(comp, "time-strip-negative" + suffix);
            set(fx, "Time Delta (frames)", 12);
        }
        app.project.bitsPerChannel = 16;
        save(source, "source-last-16", 5-1/24); save(source, "source-loop-16", 4.5); save(source, "source-ping-16", 2*(5-1/24)-9.5);
        set(fx, "Pattern Mode", 1); set(fx, "Iterations", 4); set(fx, "Time Delta (frames)", 60);
        set(fx, "Time Edge", 2); save(comp, "time-edge-held-16");
        set(fx, "Time Edge", 3); save(comp, "time-edge-loop-16");
        set(fx, "Time Edge", 4); save(comp, "time-edge-ping-16");
        set(fx, "Time Edge", 1); save(comp, "time-edge-transparent-16");
        set(fx, "Time Edge", 2); set(fx, "Time Delta (frames)", 12); set(fx, "Pattern Mode", 3);
        solid.property("ADBE Transform Group").property("ADBE Opacity").expression = "time < 2.25 ? 0 : 100";
        save(comp, "time-empty-current-16");
        solid.property("ADBE Transform Group").property("ADBE Opacity").expression = "100";
        comp.resolutionFactor = [2,2]; save(comp, "time-half-resolution-16"); comp.resolutionFactor = [1,1];
        set(fx, "Pattern Mode", 3); set(fx, "Width (px)", 40);
        set(fx, "Delta Angle", Math.atan(0.5)*180/Math.PI); set(fx, "Copies Each Side", 2);
        for (var d = 0; d < depths.length; d++) {
            app.project.bitsPerChannel = depths[d];
            save(comp, "boundary-time-" + depths[d]);
            set(fx, "Time Origin (i = 0)", [168,146]); save(comp, "boundary-time-origin-" + depths[d]);
            set(fx, "Time Origin (i = 0)", [128,96]);
        }
        set(fx, "Copies Each Side", 1);
        for (var d = 0; d < depths.length; d++) {
            app.project.bitsPerChannel = depths[d];
            set(fx, "Split Overlaps", 1); save(comp, "overlap-time-on-" + depths[d]);
            set(fx, "Split Overlaps", 0); save(comp, "overlap-time-off-" + depths[d]);
            set(fx, "Time Origin (i = 0)", [168,146]); save(comp, "overlap-time-origin-off-" + depths[d]);
            set(fx, "Time Origin (i = 0)", [128,96]);
        }
        set(fx, "Split Overlaps", 1);
        set(fx, "Delta Angle", 0);
        set(fx, "Delta X (px)", 24); set(fx, "Delta Y (px)", -12);
        for (var d = 0; d < depths.length; d++) {
            app.project.bitsPerChannel = depths[d];
            save(comp, "xy-time-" + depths[d]);
            set(fx, "Time Origin (i = 0)", [128,136]); save(comp, "xy-time-origin-" + depths[d]);
            set(fx, "Time Origin (i = 0)", [128,96]);
        }
        set(fx, "Delta X (px)", 0); set(fx, "Delta Y (px)", 0);
        // Detailed spatial source makes mask-only and image deformation visible.
        var grid = app.project.items.addComp("__IP04_Grid", 256, 192, 1, 5, 24); created.push(grid);
        for (var y = 0; y < 6; y++) { for (var x = 0; x < 8; x++) {
            var cell = grid.layers.addSolid([(x+1)/8,(y+1)/6,((x+y)%2)], "Cell", 32, 32, 1, 5); created.push(cell.source);
            cell.property("ADBE Transform Group").property("ADBE Position").setValue([x*32+16,y*32+16]);
        } }
        layer.replaceSource(grid, false); app.project.bitsPerChannel = 16; set(fx, "Time Delta (frames)", 0);
        set(fx, "Pattern Mode", 4); set(fx, "Tile Width (px)", 100); set(fx, "Tile Height (px)", 90);
        set(fx, "Rotation", 23); set(fx, "Scale X (%)", 170); set(fx, "Scale Y (%)", 65); set(fx, "Skew (deg)", 30);
        for (var shape = 1; shape <= 3; shape++) { set(fx, "Shape", shape); for (var affect = 0; affect <= 100; affect += 50) { set(fx, "Image Affect (%)", affect); save(comp, "affect-tile-" + shape + "-" + affect); } }
        for (var p = 1; p <= fx.numProperties; p++) { if (fx.property(p).name === "Repeat") { fx.property(p).setValue(2); } }
        set(fx, "Image Affect (%)", 0);
        for (var shape = 1; shape <= 3; shape++) { set(fx, "Shape", shape); save(comp, "affect-mirror-tile-" + shape); }
        set(fx, "Pattern Mode", 3); set(fx, "Angle", 34); set(fx, "Width (px)", 42);
        set(fx, "Strip Rotation", 12); set(fx, "Strip Scale X (%)", 140); set(fx, "Strip Scale Y (%)", 80); set(fx, "Strip Skew (deg)", 25);
        for (var affect = 0; affect <= 100; affect += 50) { set(fx, "Strip Image Affect (%)", affect); save(comp, "affect-strip-" + affect); }
        set(fx, "Strip Rotation", 0); set(fx, "Strip Scale X (%)", 100); set(fx, "Strip Scale Y (%)", 100); set(fx, "Strip Skew (deg)", 0);
        for (var p = 1; p <= fx.numProperties; p++) { if (fx.property(p).name === "Repeat") { fx.property(p).setValue(1); } }
        set(fx, "Angle", 45); set(fx, "Width (px)", 160);
        set(fx, "Edge Direction", 1); save(comp, "edge-along-strip"); set(fx, "Edge Direction", 2); save(comp, "edge-image-axes"); set(fx, "Edge Direction", 1);
        set(fx, "Angle", 0); set(fx, "Width (px)", 28); set(fx, "Delta Angle", 7); set(fx, "Delta Width (px)", 3); set(fx, "Copies Each Side", 6);
        for (var mode = 1; mode <= 5; mode++) { set(fx, "Delta Mode", mode); save(comp, "delta-strip-" + mode); }
        for (var p = 1; p <= fx.numProperties; p++) { if (fx.property(p).name === "Repeat") { fx.property(p).setValue(2); } }
        set(fx, "Delta Mode", 1); set(fx, "Copies Each Side", 32); set(fx, "Delta Width (px)", 0);
        for (var d = 0; d < depths.length; d++) {
            app.project.bitsPerChannel = depths[d];
            save(comp, "boundary-strip-mirror-" + depths[d]);
        }
        set(fx, "Split Overlaps", 0);
        for (var d = 0; d < depths.length; d++) {
            app.project.bitsPerChannel = depths[d];
            save(comp, "overlap-strip-off-" + depths[d]);
        }
        set(fx, "Split Overlaps", 1);
        // Image panning must leave the slit partition and temporal indices intact.
        set(fx, "Angle", 0); set(fx, "Width (px)", 40); set(fx, "Delta Angle", 0);
        set(fx, "Delta Cycle / Range", 2); set(fx, "Edge Direction", 2); set(fx, "Edge", 1);
        for (var d = 0; d < depths.length; d++) {
            app.project.bitsPerChannel = depths[d]; var suffix = "-" + depths[d];
            save(grid, "xy-source" + suffix);
            for (var p = 1; p <= fx.numProperties; p++) { if (fx.property(p).name === "Repeat") { fx.property(p).setValue(1); } }
            set(fx, "Delta Mode", 1); set(fx, "Delta X (px)", 0); set(fx, "Delta Y (px)", 0);
            save(comp, "xy-zero" + suffix);
            set(fx, "Delta X (px)", 12); save(comp, "xy-x" + suffix);
            set(fx, "Delta X (px)", 0); set(fx, "Delta Y (px)", -8); save(comp, "xy-y" + suffix);
            set(fx, "Delta X (px)", 12);
            for (var mode = 1; mode <= 4; mode++) { set(fx, "Delta Mode", mode); save(comp, "xy-mode-" + mode + suffix); }
            set(fx, "Delta Mode", 5); set(fx, "Random Seed", 7); save(comp, "xy-random" + suffix);
            set(fx, "Random Seed", 19); save(comp, "xy-random-other" + suffix);
            set(fx, "Random Seed", 7); save(comp, "xy-random-repeat" + suffix);
            set(fx, "Delta Mode", 1);
            for (var p = 1; p <= fx.numProperties; p++) { if (fx.property(p).name === "Repeat") { fx.property(p).setValue(2); } }
            save(comp, "xy-mirror" + suffix);
            for (var p = 1; p <= fx.numProperties; p++) { if (fx.property(p).name === "Repeat") { fx.property(p).setValue(1); } }
            set(fx, "Angle", 90); save(comp, "xy-angle" + suffix); set(fx, "Angle", 0);
            set(fx, "Delta Angle", Math.atan(0.5)*180/Math.PI); set(fx, "Copies Each Side", 1); set(fx, "Edge", 2);
            for (var split = 0; split <= 1; split++) {
                set(fx, "Split Overlaps", split);
                set(fx, "Delta X (px)", 0); set(fx, "Delta Y (px)", 0); save(comp, "xy-mask-zero-" + split + suffix);
                set(fx, "Delta X (px)", 12); set(fx, "Delta Y (px)", -8); save(comp, "xy-mask-pan-" + split + suffix);
            }
            set(fx, "Delta Angle", 0); set(fx, "Split Overlaps", 1); set(fx, "Edge", 1);
        }
        app.project.save(new File(output.fsName + "/verification.aep"));
        log("RESULT=PASS");
    } catch (error) { log("RESULT=FAIL " + error + " line " + error.line); }
    finally {
        app.project.bitsPerChannel = depth;
        for (var i = created.length - 1; i >= 0; i--) { try { created[i].remove(); } catch (ignored) {} }
        report.close();
    }
}());
