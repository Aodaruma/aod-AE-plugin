// SPDX-License-Identifier: MPL-2.0
// Run in an empty project after smoke.ps1 has generated the source/map fixtures.
(function () {
    $.evalFile(new File(File($.fileName).parent.fsName + "/controls.jsxinc"));
    var root = new Folder(File($.fileName).parent.parent.parent.parent.fsName + "/target/codec-map-gop-one");
    root.create();
    var log = new File(root.fsName + "/host-report.txt");
    function note(s) { log.open("a"); log.writeln(s); log.close(); }
    try {
        if (app.project.numItems !== 0) { throw new Error("GOP=1 test needs an empty project"); }
        log.open("w"); log.close();
        var fixtures = root.parent.fsName + "/codec-map-smoke/";
        var source = app.project.importFile(new ImportOptions(new File(fixtures + "source.png")));
        var map = app.project.importFile(new ImportOptions(new File(fixtures + "map.png")));
        var comp = app.project.items.addComp("CodecMap GOP=1", 129, 97, 1, 2, 24);
        var layer = comp.layers.add(source);
        var mapLayer = comp.layers.add(map); mapLayer.enabled = false;
        var fx = layer.property("ADBE Effect Parade").addProperty("CodecMap");
        codecParam(fx, "Quality Control").setValue(2);
        codecParam(fx, "GOP Length").setValue(1);
        codecParam(fx, "Map Source").setValue(2);
        codecParam(fx, "Compression Map").setValue(mapLayer.index);
        var order = [0, 5, 2, 1, 11];
        for (var mapped = 1; mapped <= 2; mapped++) {
            codecParam(fx, "Map Source").setValue(mapped);
            for (var mode = 1; mode <= 2; mode++) {
                codecParam(fx, "Temporal Mode").setValue(mode);
                for (var pass = 0; pass < 2; pass++) {
                    app.purge(PurgeTarget.ALL_CACHES);
                    for (var i = 0; i < order.length; i++) {
                        var name = "map" + mapped + "-mode" + mode + "-pass" + pass + "-frame" + order[i];
                        comp.time = order[i] / 24;
                        comp.saveFrameToPng(comp.time, new File(root.fsName + "/" + name + ".png"));
                        note("requested " + name);
                    }
                }
            }
        }
        codecParam(fx, "Temporal Mode").setValue(1);
        comp.openInViewer();
        app.project.save(new File(root.fsName + "/gop-one.aep"));
        note("Requests completed: AE " + app.version + ". Verify the 40 PNGs externally.");
    } catch (e) { note("FAIL " + e.toString()); throw e; }
})();
