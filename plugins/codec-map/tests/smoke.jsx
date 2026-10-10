// SPDX-License-Identifier: MPL-2.0
// Run in an empty, dedicated AE instance. Input fixtures are made by smoke.ps1.
(function () {
    var root = new Folder(File($.fileName).parent.parent.parent.parent.fsName + "/target/codec-map-smoke");
    var log = new File(root.fsName + "/host-report.txt");
    log.open("w"); log.close();
    function note(s) { log.open("a"); log.writeln(s); log.close(); }
    function save(comp, name, frame) {
        var file = new File(root.fsName + "/" + name + ".png");
        if (file.exists && !file.remove()) { throw new Error("Cannot replace " + file.fsName); }
        comp.saveFrameToPng(frame / 24, file);
        // PNG writes can finish after the JSX call; smoke.ps1 verifies the files.
        note("requested " + name);
    }
    app.beginSuppressDialogs();
    try {
        if (app.project.numItems !== 0) { throw new Error("Smoke test needs an empty project"); }
        note("AE " + app.version);
        app.project.bitsPerChannel = 8;
        var comp = app.project.items.addComp("CodecMap Smoke", 129, 97, 1, 2, 24);
        var input = app.project.importFile(new ImportOptions(new File(root.fsName + "/source.png")));
        var map = app.project.importFile(new ImportOptions(new File(root.fsName + "/map.png")));
        var layer = comp.layers.add(input);
        var mapLayer = comp.layers.add(map); mapLayer.enabled = false;
        var fx = layer.property("ADBE Effect Parade").addProperty("CodecMap");
        if (!fx) { throw new Error("CodecMap was not loaded"); }
        note("effect " + fx.name + " / " + fx.matchName + " / " + fx.numProperties + " params");
        fx.property("Map Source").setValue(2);
        fx.property("Compression Map").setValue(mapLayer.index);
        fx.property("Base CRF").setValue(35);
        fx.property("White QP Offset").setValue(20);
        fx.enabled = false; save(comp, "reference", 5); fx.enabled = true;
        save(comp, "isolated8", 5);
        save(comp, "order1", 1);
        app.purge(PurgeTarget.ALL_CACHES);
        save(comp, "replay8", 5);
        fx.property("Output").setValue(2); save(comp, "full8", 5);
        fx.property("Temporal Mode").setValue(2); save(comp, "independent8", 5);
        fx.property("Temporal Mode").setValue(1);
        fx.property("Preview").setValue(2); save(comp, "map-preview", 5);
        fx.property("Preview").setValue(3); save(comp, "offset-preview", 5);
        fx.property("Preview").setValue(1);
        fx.property("Output").setValue(1);
        fx.property("Mix").setValue(0); save(comp, "zero-mix8", 5);
        fx.property("Mix").setValue(100);
        fx.property("Compression Map").setValue(0); save(comp, "missing-map8", 5);
        fx.property("Compression Map").setValue(mapLayer.index);
        fx.property("Invert Map").setValue(1); save(comp, "inverted8", 5);
        fx.property("Invert Map").setValue(0);
        fx.property("White QP Offset").setValueAtTime(0, -20);
        fx.property("White QP Offset").setValueAtTime(4 / 24, 20);
        save(comp, "past-map-edit8", 5);
        for (var i = fx.property("White QP Offset").numKeys; i > 0; i--) { fx.property("White QP Offset").removeKey(i); }
        fx.property("White QP Offset").setValue(20);
        for (var b = 16; b <= 32; b *= 2) {
            app.project.bitsPerChannel = b;
            fx.enabled = false; save(comp, "reference" + b, 5); fx.enabled = true;
            save(comp, "isolated" + b, 5);
            fx.property("Mix").setValue(0); save(comp, "zero-mix" + b, 5);
            fx.property("Mix").setValue(100);
        }
        app.project.bitsPerChannel = 8;
        comp.resolutionFactor = [2, 2]; save(comp, "half-resolution", 5);
        comp.resolutionFactor = [1, 1];
        app.project.save(new File(root.fsName + "/codec-map-smoke.aep"));
        note("Host render calls completed; run smoke.ps1 -ValidateOnly to verify output");
    } catch (e) { note("FAIL " + e.toString() + " line " + e.line); }
    finally { app.endSuppressDialogs(false); log.close(); }
}());
