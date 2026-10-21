// Run only in a new, isolated After Effects process: AfterFX -m -noui -r <this file>.
(function () {
    var root = File($.fileName).parent.parent.parent.parent;
    var output = new Folder(root.fsName + "/target/ae-smoke");
    output.create();
    var report = new File(output.fsName + "/report.txt");
    report.open("w");
    function log(text) { report.writeln(text); report.close(); report.open("a"); }
    if (app.project && (app.project.numItems > 0 || app.project.file)) {
        log("REFUSED: a nonempty project is open."); report.close(); return;
    }
    app.beginSuppressDialogs();
    try {
        var source = app.project.items.addComp("ImageResize Source", 80, 40, 1, 1, 30);
        var colors = [[1,0,0], [0,1,0], [0,0,1], [1,1,0]];
        for (var i = 0; i < 4; i++) {
            var stripe = source.layers.addSolid(colors[i], "Stripe " + i, 20, 40, 1, 1);
            stripe.property("ADBE Transform Group").property("ADBE Position").setValue([10 + 20*i, 20]);
        }
        var cases = [
            ["baseline", null], ["identity", {}],
            ["crop", {"Left":25, "Top":25}],
            ["pixels", {"Units":1, "Left":20, "Top":10}],
            ["all50", {"Left":25, "Top":25, "All %":50}],
            ["all0", {"Left":-80, "Top":90, "All %":0}],
            ["none", {"Left":-25, "Top":-25, "Right":-25, "Bottom":-25}],
            ["mirror", {"Left":-25, "Top":-25, "Right":-25, "Bottom":-25, "Extend":2}],
            ["tile", {"Left":-25, "Top":-25, "Right":-25, "Bottom":-25, "Extend":3}],
            ["clamp", {"Left":-25, "Top":-25, "Right":-25, "Bottom":-25, "Extend":4}],
            ["empty", {"Left":100}],
            ["rotate", {"Angle":90}],
            ["rotate_all0", {"Angle":90, "Left":25, "All %":0}],
            ["rotate27", {"Angle":27, "Left":-25, "Extend":2}],
            ["fractional", {"Units":1, "Left":0.25}],
            ["size_disabled", {"Output Width (px)":40, "Output Height (px)":20}],
            ["size_crop", {"Set Output Size":1, "Output Width (px)":40, "Output Height (px)":20}],
            ["size_shift", {"Set Output Size":1, "Output Width (px)":40, "Output Height (px)":20, "Left":25}],
            ["size_all0", {"Set Output Size":1, "Output Width (px)":40, "Output Height (px)":20, "Left":25, "All %":0}],
            ["size_odd", {"Set Output Size":1, "Output Width (px)":41, "Output Height (px)":21}],
            ["size_none", {"Set Output Size":1, "Output Width (px)":120, "Output Height (px)":60}],
            ["size_mirror", {"Set Output Size":1, "Output Width (px)":120, "Output Height (px)":60, "Extend":2}],
            ["size_tile", {"Set Output Size":1, "Output Width (px)":120, "Output Height (px)":60, "Extend":3}],
            ["size_clamp", {"Set Output Size":1, "Output Width (px)":120, "Output Height (px)":60, "Extend":4}],
            ["size_rotate", {"Set Output Size":1, "Output Width (px)":40, "Output Height (px)":80, "Angle":90}],
            ["size_rotate_crop", {"Set Output Size":1, "Output Width (px)":20, "Output Height (px)":60, "Angle":90}],
            ["size_rotate_extend", {"Set Output Size":1, "Output Width (px)":60, "Output Height (px)":120, "Angle":90, "Extend":2}],
            ["rotate_extend", {"Left":-25, "Top":-25, "Right":-25, "Bottom":-25, "Angle":90, "Extend":2}]
        ];
        for (var depthIndex = 0; depthIndex < 3; depthIndex++) {
            var depth = [8,16,32][depthIndex];
            app.project.bitsPerChannel = depth;
            for (var c = 0; c < cases.length; c++) {
                var name = cases[c][0] + "-" + depth;
                var comp = app.project.items.addComp(name, 200, 160, 1, 1, 30);
                var layer = comp.layers.add(source);
                if (cases[c][1] !== null) {
                    var fx = layer.property("ADBE Effect Parade").addProperty("ImageResize");
                    if (!fx) { throw new Error("ImageResize not loaded"); }
                    var values = cases[c][1];
                    for (var key in values) { fx.property(key).setValue(values[key]); }
                    if (c === 1 && depth === 8) {
                        for (var pi = 1; pi <= fx.numProperties; pi++) {
                            log("PARAM " + pi + " " + fx.property(pi).name + "=" + fx.property(pi).value);
                        }
                    }
                }
                comp.saveFrameToPng(0, new File(output.fsName + "/" + name + ".png"));
                log("RENDERED " + name);
            }
        }
        app.project.bitsPerChannel = 8;
        var half = app.project.items.addComp("half", 200, 160, 1, 1, 30);
        half.resolutionFactor = [2,2];
        var halfLayer = half.layers.add(source);
        var halfFx = halfLayer.property("ADBE Effect Parade").addProperty("ImageResize");
        halfFx.property("Left").setValue(25);
        halfFx.property("Top").setValue(25);
        half.saveFrameToPng(0, new File(output.fsName + "/half.png"));
        log("RENDERED half");
        halfFx.property("Set Output Size").setValue(1);
        halfFx.property("Output Width (px)").setValue(40);
        halfFx.property("Output Height (px)").setValue(20);
        halfFx.property("Top").setValue(0);
        half.saveFrameToPng(0, new File(output.fsName + "/half-size.png"));
        log("RENDERED half-size");
        var chained = app.project.items.addComp("chained", 200, 160, 1, 1, 30);
        var chainedLayer = chained.layers.add(source);
        var first = chainedLayer.property("ADBE Effect Parade").addProperty("ImageResize");
        first.property("Left").setValue(25);
        var second = chainedLayer.property("ADBE Effect Parade").addProperty("ImageResize");
        second.property("Left").setValue(-25);
        second.property("Right").setValue(-25);
        second.property("Extend").setValue(3);
        chained.saveFrameToPng(0, new File(output.fsName + "/chained.png"));
        log("RENDERED chained");
        app.project.save(new File(output.fsName + "/image-resize-smoke.aep"));
        log("PASS: all cases rendered with " + app.version);
    } catch (error) {
        log("FAIL: " + error.toString() + " line=" + error.line);
    } finally {
        report.close();
        app.endSuppressDialogs(false);
        app.project.close(CloseOptions.DO_NOT_SAVE_CHANGES);
        if (typeof AOD_IMAGE_RESIZE_KEEP_OPEN === "undefined" || !AOD_IMAGE_RESIZE_KEEP_OPEN) { app.quit(); }
    }
}());
