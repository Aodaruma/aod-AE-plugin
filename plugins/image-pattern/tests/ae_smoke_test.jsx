(function () {
    var root = new Folder(File($.fileName).parent.parent.parent.parent.fsName);
    var output = new Folder(root.fsName + "/target/ae-image-pattern-smoke");
    if (!output.exists && !output.create()) { throw new Error("Cannot create " + output.fsName); }
    var report = new File(output.fsName + "/report.txt");
    report.encoding = "UTF-8";
    report.open("w");
    function log(text) { report.writeln(text); report.close(); report.open("a"); }
    function property(effect, name) {
        var p = effect.property(name);
        if (!p) { throw new Error("Missing control: " + name); }
        return p;
    }
    function set(effect, name, value) { property(effect, name).setValue(value); }
    function equal(actual, expected, name) {
        if (actual !== expected) { throw new Error(name + ": " + actual + " != " + expected); }
    }
    function save(comp, name) {
        var file = new File(output.fsName + "/" + name + ".png");
        if (file.exists && !file.remove()) { throw new Error("Cannot replace test frame: " + file.fsName); }
        comp.saveFrameToPng(0, file);
        var deadline = new Date().getTime() + 10000;
        do {
            file = new File(file.fsName);
            if (file.exists && file.length > 0) { return file; }
            $.sleep(25);
        } while (new Date().getTime() < deadline);
        throw new Error("No render: " + file.fsName);
    }
    function repeat(effect, value) {
        for (var p = 1; p <= effect.numProperties; p++) {
            if (effect.property(p).name === "Repeat") { effect.property(p).setValue(value); }
        }
    }
    var created = [];
    var originalBpc = app.project.bitsPerChannel;
    var originalActive = app.project.activeItem;
    app.beginUndoGroup("ImagePattern smoke test");
    try {
        log("After Effects " + app.version);
        var source = app.project.items.addComp("__ImagePattern_Source", 512, 512, 1, 1, 30);
        created.push(source);
        for (var y = 0; y < 8; y++) {
            for (var x = 0; x < 8; x++) {
                var cell = source.layers.addSolid([
                    ((x * 37 + y * 11) % 101) / 100,
                    ((x * 13 + y * 47 + 19) % 101) / 100,
                    ((x * 61 + y * 23 + 7) % 101) / 100
                ], "Cell", 64, 64, 1, 1);
                created.push(cell.source);
                cell.property("ADBE Transform Group").property("ADBE Position").setValue([x * 64 + 32, y * 64 + 32]);
            }
        }
        var comp = app.project.items.addComp("__ImagePattern_Test", 512, 512, 1, 1, 30);
        created.push(comp);
        var layer = comp.layers.add(source);
        var effect = layer.property("ADBE Effect Parade").addProperty("ImagePattern");
        equal(effect.name, "AOD_ImagePattern", "Display name");
        equal(property(effect, "Decay").value, 1, "Default decay");
        equal(property(effect, "Edge").value, 1, "Default edge");
        equal(property(effect, "Pattern Mode").maxValue, 4, "Mode count");
        equal(property(effect, "Shape").maxValue, 3, "Shape count");
        equal(property(effect, "Delta X (px)").value, 0, "Default Delta X");
        equal(property(effect, "Delta Y (px)").value, 0, "Default Delta Y");
        equal(property(effect, "Clipping").value, 1, "Default clipping");
        equal(property(effect, "Clipping").maxValue, 4, "Clipping choices");
        equal(property(effect, "Overlap Order").value, 1, "Default overlap order");
        for (var p = 1; p <= effect.numProperties; p++) {
            var name = effect.property(p).name;
            if (/Use Layer Center|Wave|Complex|Mobius|Reciprocal|Exponent/.test(name)) { throw new Error("Removed control remains: " + name); }
        }
        equal(property(effect, "Center").value[0], 256, "Default Center X");
        equal(property(effect, "Center").value[1], 256, "Default Center Y");
        log("PASS display name, defaults, modes, shapes, removed controls");
        var cases = [
            { name: "feedback", mode: 1 },
            { name: "polar", mode: 2 },
            { name: "strip-tile", mode: 3, repeat: 1 },
            { name: "strip-mirror", mode: 3, repeat: 2 }
        ];
        var shapes = ["triangle", "quadrilateral", "hexagon"];
        for (var s = 1; s <= 3; s++) {
            for (var r = 1; r <= 2; r++) {
                cases.push({ name: shapes[s - 1] + (r === 1 ? "-tile" : "-mirror"), mode: 4, shape: s, repeat: r });
            }
        }
        var depths = [8, 16, 32];
        for (var d = 0; d < depths.length; d++) {
            app.project.bitsPerChannel = depths[d];
            for (var c = 0; c < cases.length; c++) {
                var item = cases[c];
                set(effect, "Pattern Mode", item.mode);
                set(effect, "Center", [270, 245]);
                if (item.mode === 3) {
                    set(effect, "Angle", 31);
                    set(effect, "Width (px)", 83);
                    repeat(effect, item.repeat);
                } else if (item.mode === 4) {
                    set(effect, "Shape", item.shape);
                    set(effect, "Tile Width (px)", 147);
                    set(effect, "Tile Height (px)", 163);
                    set(effect, "Rotation", 19);
                    set(effect, "Scale X (%)", 83);
                    set(effect, "Scale Y (%)", 121);
                    set(effect, "Skew (deg)", 27);
                    set(effect, "Skew Axis", -17);
                    repeat(effect, item.repeat);
                }
                var file = save(comp, item.name + "-" + depths[d]);
                log("PASS " + file.name);
            }
        }
        set(effect, "Mix (%)", 0);
        save(comp, "mix-zero");
        save(source, "source");
        log("RESULT=PASS");
    } catch (error) {
        log("RESULT=FAIL " + error.toString() + " line " + error.line);
    } finally {
        app.project.bitsPerChannel = originalBpc;
        for (var i = created.length - 1; i >= 0; i--) {
            try { created[i].remove(); } catch (ignored) {}
        }
        try { if (originalActive && originalActive.openInViewer) { originalActive.openInViewer(); } } catch (ignoredRestore) {}
        app.endUndoGroup();
        report.close();
    }
}());
