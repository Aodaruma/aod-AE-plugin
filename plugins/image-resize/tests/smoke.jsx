// Run only in a new, isolated After Effects process: AfterFX -m -noui -r <this file>.
(function () {
    var root = File($.fileName).parent.parent.parent.parent;
    var output = new Folder(root.fsName + "/target/ae-smoke");
    output.create();
    var report = new File(output.fsName + "/report.txt");
    report.open("w");
    function log(text) { report.writeln(text); report.close(); report.open("a"); }
    function saveLegacyFrame(comp, destination) {
        // saveFrameToPng can silently skip a frame just after opening a project.
        if (destination.exists && !destination.remove()) { throw new Error("Cannot replace " + destination.fsName); }
        comp.saveFrameToPng(0, destination);
        if (destination.exists && destination.length > 0) { return; }
        var queue = app.project.renderQueue;
        var enabled = [];
        var item = null;
        try {
            for (var q = 1; q <= queue.numItems; q++) {
                if (queue.item(q).render) { enabled.push(queue.item(q)); queue.item(q).render = false; }
            }
            item = queue.items.add(comp);
            item.timeSpanStart = 0;
            item.timeSpanDuration = comp.frameDuration;
            var module = item.outputModule(1);
            var template = null;
            for (var t = 0; t < module.templates.length; t++) {
                if (/PNG/i.test(module.templates[t])) { template = module.templates[t]; break; }
            }
            if (!template) { throw new Error("PNG output template required for fixture render"); }
            module.applyTemplate(template);
            var prefix = "legacy-export-" + new Date().getTime();
            module.file = new File(output.fsName + "/" + prefix + "-[#####].png");
            queue.render();
            var frames = output.getFiles(prefix + "-*.png");
            if (item.status !== RQItemStatus.DONE || frames.length !== 1 || !frames[0].copy(destination.fsName)) {
                throw new Error("Failed to render " + comp.name);
            }
            frames[0].remove();
        } finally {
            if (item) { item.remove(); }
            for (var e = 0; e < enabled.length; e++) { enabled[e].render = true; }
        }
    }
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
            ["pixels", {"Mode":1, "Left":20, "Top":10}],
            ["all50", {"Left":25, "Top":25, "All %":50}],
            ["all0", {"Left":-80, "Top":90, "All %":0}],
            ["all300", {"Left":10, "Top":10, "All %":300}],
            ["all300_reference", {"Left":30, "Top":30}],
            ["all_negative", {"Left":25, "Top":25, "All %":-100, "Extend":2}],
            ["all_negative_reference", {"Left":-25, "Top":-25, "Extend":2}],
            ["all_negative_crop", {"Left":-25, "Top":-25, "All %":-100}],
            ["pixels_all300", {"Mode":1, "Left":5, "Top":2, "All %":300}],
            ["pixels_all_negative", {"Mode":1, "Left":20, "Top":10, "All %":-100, "Extend":2}],
            ["none", {"Left":-25, "Top":-25, "Right":-25, "Bottom":-25}],
            ["mirror", {"Left":-25, "Top":-25, "Right":-25, "Bottom":-25, "Extend":2}],
            ["tile", {"Left":-25, "Top":-25, "Right":-25, "Bottom":-25, "Extend":3}],
            ["clamp", {"Left":-25, "Top":-25, "Right":-25, "Bottom":-25, "Extend":4}],
            ["empty", {"Left":100}],
            ["rotate", {"Angle":90}],
            ["rotate_all0", {"Angle":90, "Left":25, "All %":0}],
            ["rotate27", {"Angle":27, "Left":-25, "Extend":2}],
            ["fractional", {"Mode":1, "Left":0.25}],
            ["size_disabled", {"Output Width (px)":40, "Output Height (px)":20}],
            ["size_crop", {"Mode":3, "Output Width (px)":40, "Output Height (px)":20}],
            ["size_shift", {"Mode":3, "Output Width (px)":40, "Output Height (px)":20, "Left":25}],
            ["size_all0", {"Mode":3, "Output Width (px)":40, "Output Height (px)":20, "Left":25, "All %":0}],
            ["size_odd", {"Mode":3, "Output Width (px)":41, "Output Height (px)":21}],
            ["size_none", {"Mode":3, "Output Width (px)":120, "Output Height (px)":60}],
            ["size_mirror", {"Mode":3, "Output Width (px)":120, "Output Height (px)":60, "Extend":2}],
            ["size_tile", {"Mode":3, "Output Width (px)":120, "Output Height (px)":60, "Extend":3}],
            ["size_clamp", {"Mode":3, "Output Width (px)":120, "Output Height (px)":60, "Extend":4}],
            ["size_rotate", {"Mode":3, "Output Width (px)":40, "Output Height (px)":80, "Angle":90}],
            ["size_rotate_crop", {"Mode":3, "Output Width (px)":20, "Output Height (px)":60, "Angle":90}],
            ["size_rotate_extend", {"Mode":3, "Output Width (px)":60, "Output Height (px)":120, "Angle":90, "Extend":2}],
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
                        if (fx.property("Mode").maxValue !== 3) { throw new Error("Mode must have three choices"); }
                        if (fx.property("All %").minValue > -300 || fx.property("All %").maxValue < 300) { throw new Error("All % range is too narrow"); }
                        if (fx.property("Set Output Size") !== null) { throw new Error("Old output-size checkbox still present"); }
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
        halfFx.property("Mode").setValue(3);
        halfFx.property("Output Width (px)").setValue(40);
        halfFx.property("Output Height (px)").setValue(20);
        halfFx.property("Top").setValue(0);
        half.saveFrameToPng(0, new File(output.fsName + "/half-size.png"));
        log("RENDERED half-size");
        half.resolutionFactor = [1,1];
        halfFx.property("Mode").setValue(1);
        halfFx.property("Left").setValue(20);
        halfFx.property("Top").setValue(10);
        half.saveFrameToPng(0, new File(output.fsName + "/switch-pixels.png"));
        halfFx.property("Mode").setValue(2);
        halfFx.property("Left").setValue(25);
        halfFx.property("Top").setValue(25);
        half.saveFrameToPng(0, new File(output.fsName + "/switch-percent.png"));
        halfFx.property("Mode").setValue(3);
        halfFx.property("Top").setValue(0);
        half.saveFrameToPng(0, new File(output.fsName + "/switch-size.png"));
        log("RENDERED mode switches");
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
        // Optional local fixture saved with v0.1 before replacing the binary.
        var oldProject = new File(output.fsName + "/image-resize-v0.1.aep");
        if (oldProject.exists) {
            app.project.close(CloseOptions.DO_NOT_SAVE_CHANGES);
            app.open(oldProject);
            app.project.bitsPerChannel = 8;
            var legacyCount = 0;
            for (var li = 1; li <= app.project.numItems; li++) {
                var oldComp = app.project.item(li);
                if (!(oldComp instanceof CompItem)) { continue; }
                if (oldComp.name !== "pixels-8" && oldComp.name !== "size_shift-8") { continue; }
                var oldFx = oldComp.layer(1).property("ADBE Effect Parade").property(1);
                if (oldComp.name === "pixels-8") {
                    if (oldFx.property("Mode").value !== 1 || oldFx.property("Left").value !== 20) { throw new Error("Old Pixels settings changed"); }
                } else {
                    if (oldFx.property("Output Width (px)").value !== 40 || oldFx.property("Output Height (px)").value !== 20) { throw new Error("Old output dimensions changed"); }
                    oldFx.property("Mode").setValue(3);
                }
                saveLegacyFrame(oldComp, new File(output.fsName + "/legacy-" + oldComp.name + ".png"));
                legacyCount++;
            }
            if (legacyCount !== 2) { throw new Error("Incomplete v0.1 fixture"); }
            log("PASS: v0.1 parameter persistence");
        }
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
