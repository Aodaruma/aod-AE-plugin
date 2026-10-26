(function () {
    var root = new Folder(File($.fileName).parent.parent.parent.parent.fsName);
    var output = new Folder(root.fsName + "/target/ae-image-pattern-clipping");
    if (!output.exists) { output.create(); }
    var report = new File(output.fsName + "/report.txt"); report.encoding = "UTF-8"; report.open("w");
    function log(s) { report.writeln(s); report.close(); report.open("a"); }
    function set(name, value) { var p = fx.property(name); if (!p) { throw new Error("Missing " + name); } p.setValue(value); }
    function repeat(value) { for (var p = 1; p <= fx.numProperties; p++) { if (fx.property(p).name === "Repeat") { fx.property(p).setValue(value); } } }
    function save(comp, name, time) {
        var file = new File(output.fsName + "/" + name + ".png"); if (file.exists) { file.remove(); }
        comp.saveFrameToPng(time === undefined ? 2 : time, file);
        var deadline = new Date().getTime() + 30000;
        do { file = new File(file.fsName); if (file.exists && file.length > 0) { log("PASS " + name); return; } $.sleep(25); } while (new Date().getTime() < deadline);
        throw new Error("Render timeout " + name);
    }
    var created = [], depth = app.project.bitsPerChannel;
    try {
        log("AE " + app.version);
        var source = app.project.items.addComp("__IP044_Source", 256, 192, 1, 5, 24); created.push(source);
        for (var y = 0; y < 6; y++) { for (var x = 0; x < 8; x++) {
            var cell = source.layers.addSolid([1,1,1], "Animated cell", 32, 32, 1, 5); created.push(cell.source);
            cell.property("ADBE Transform Group").property("ADBE Position").setValue([x*32+16,y*32+16]);
            cell.property("ADBE Transform Group").property("ADBE Opacity").setValue(25 + ((x+y)%3)*25);
            var fill = cell.property("ADBE Effect Parade").addProperty("ADBE Fill");
            fill.property("ADBE Fill-0002").expression = "[" + (x+1)/8 + "," + (y+1)/6 + ",time/5,1]";
        } }
        var comp = app.project.items.addComp("__IP044_Clipping", 256, 192, 1, 5, 24); created.push(comp);
        var layer = comp.layers.add(source), fx = layer.property("ADBE Effect Parade").addProperty("ImagePattern");
        if (fx.property("Clipping").value !== 1 || fx.property("Overlap Order").value !== 1) { throw new Error("Incorrect defaults"); }
        set("Pattern Mode", 3); set("Sampling", 1); set("Edge", 1); set("Edge Direction", 2);
        set("Width (px)", 40); set("Copies Each Side", 1); set("Delta X (px)", 12); set("Delta Y (px)", -8);
        var nativeRefs = [];
        for (var order = 1; order <= 2; order++) {
            var nativeComp = app.project.items.addComp("__IP044_Native_" + order, 256, 192, 1, 5, 24); created.push(nativeComp);
            for (var stack = -1; stack <= 1; stack++) {
                var index = order === 1 ? stack : -stack;
                var nativeLayer = nativeComp.layers.add(source);
                nativeLayer.property("ADBE Transform Group").property("ADBE Position").setValue([128+12*index,96+32*index]);
            }
            nativeRefs.push(nativeComp);
        }
        var depths = [8,16,32];
        for (var d = 0; d < depths.length; d++) {
            app.project.bitsPerChannel = depths[d]; var suffix = "-" + depths[d];
            for (var t = 0; t <= 9; t++) { save(source, "source-" + t + suffix, t * 0.5); }
            save(source, "source-last" + suffix, 5 - 1/24);
            for (var order = 1; order <= 2; order++) { save(nativeRefs[order-1], "native-" + order + suffix); }
            for (var r = 1; r <= 2; r++) {
                repeat(r);
                for (var clip = 1; clip <= 4; clip++) {
                    set("Clipping", clip);
                    for (var order = 1; order <= 2; order++) {
                        set("Overlap Order", order); save(comp, "static-" + r + "-" + clip + "-" + order + suffix);
                    }
                }
            }
            repeat(1); set("Time Delta (frames)", 12);
            for (var clip = 2; clip <= 4; clip++) {
                set("Clipping", clip);
                for (var order = 1; order <= 2; order++) {
                    set("Overlap Order", order); save(comp, "time-" + clip + "-" + order + suffix);
                }
            }
            set("Clipping", 4); set("Time Delta (frames)", -12);
            for (var order = 1; order <= 2; order++) { set("Overlap Order", order); save(comp, "negative-" + order + suffix); }
            set("Time Delta (frames)", 12); set("Time Origin (i = 0)", [128,136]);
            for (var order = 1; order <= 2; order++) { set("Overlap Order", order); save(comp, "origin-" + order + suffix); }
            set("Time Origin (i = 0)", [128,96]);
            set("Copies Each Side", 2); set("Time Delta (frames)", 120);
            for (var order = 1; order <= 2; order++) { set("Overlap Order", order); save(comp, "held-" + order + suffix); }
            set("Time Delta (frames)", 60); set("Time Edge", 3);
            for (var order = 1; order <= 2; order++) { set("Overlap Order", order); save(comp, "loop-" + order + suffix); }
            set("Time Edge", 2); set("Time Delta (frames)", 12); set("Copies Each Side", 1);
            set("Delta Angle", Math.atan(0.5)*180/Math.PI);
            for (var split = 0; split <= 1; split++) {
                set("Split Overlaps", split);
                for (var clip = 2; clip <= 4; clip++) {
                    set("Clipping", clip);
                    for (var order = 1; order <= 2; order++) {
                        set("Overlap Order", order); save(comp, "cross-" + split + "-" + clip + "-" + order + suffix);
                    }
                }
            }
            set("Delta Angle", 0); set("Split Overlaps", 1); set("Time Delta (frames)", 0);
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
