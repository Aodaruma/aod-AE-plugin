// SPDX-License-Identifier: MPL-2.0
// Run in an empty AE project after smoke.ps1 has generated the input fixtures.
(function () {
    $.evalFile(new File(File($.fileName).parent.fsName + "/controls.jsxinc"));
    var root = new Folder(File($.fileName).parent.parent.parent.parent.fsName + "/target/codec-map-codecs");
    root.create();
    var log = new File(root.fsName + "/host-report.txt");
    function note(s) { log.open("a"); log.writeln(s); log.close(); }
    function save(comp, name, frame) {
        comp.time = frame / 24;
        var file = new File(root.fsName + "/" + name + ".png");
        if (file.exists) {file.remove();}
        comp.saveFrameToPng(comp.time, file);
        note("requested " + name);
    }
    try {
        if (app.project.numItems !== 0) { throw new Error("Codec test needs an empty project"); }
        log.open("w"); log.close();
        app.project.bitsPerChannel = 8;
        var fixtures = root.parent.fsName + "/codec-map-smoke/";
        var source = app.project.importFile(new ImportOptions(new File(fixtures + "source.png")));
        var map = app.project.importFile(new ImportOptions(new File(fixtures + "map.png")));
        // Animate inside a fixed-size precomp so every frame has different coded pixels.
        var footage = app.project.items.addComp("Moving Source", 129, 97, 1, 2, 24);
        var image = footage.layers.add(source);
        image.property("ADBE Transform Group").property("ADBE Position").expression = "[64.5 + time*35,48.5]";
        var comp = app.project.items.addComp("CodecMap Codecs", 129, 97, 1, 2, 24);
        var layer = comp.layers.add(footage);
        var mapLayer = comp.layers.add(map); mapLayer.enabled = false;
        var fx = layer.property("ADBE Effect Parade").addProperty("CodecMap");
        codecParam(fx, "Quality Control").setValue(2);
        note("AE " + app.version + " params=" + fx.numProperties);
        var dropI = codecParam(fx, "Drop I-Frames"), dropB = codecParam(fx, "Drop B-Frames");
        if (!dropI.canVaryOverTime || !dropB.canVaryOverTime) { throw new Error("Drop controls cannot animate"); }
        codecParam(fx, "GOP Length").setValue(6);
        codecParam(fx, "Output").setValue(2);
        codecParam(fx, "Map Source").setValue(2);
        codecParam(fx, "Compression Map").setValue(mapLayer.index);
        codecParam(fx, "Map Source").setValue(1);
        for (var codec = 1; codec <= 3; codec++) {
            codecParam(fx, "Codec").setValue(codec);
            save(comp, "codec" + codec + "-normal", 8);
            dropI.setValueAtTime(0, 0);
            dropI.setValueAtTime(6 / 24, 1);
            dropI.setValueAtTime(12 / 24, 0);
            save(comp, "codec" + codec + "-drop-i", 8);
            app.purge(PurgeTarget.ALL_CACHES);
            save(comp, "codec" + codec + "-other-time", 2);
            save(comp, "codec" + codec + "-replay-i", 8);
            save(comp, "codec" + codec + "-reset-i", 13);
            while (dropI.numKeys) { dropI.removeKey(dropI.numKeys); }
            dropI.setValue(0);
            save(comp, "codec" + codec + "-normal13", 13);
            codecParam(fx, "GOP Length").setValue(1);
            save(comp, "codec" + codec + "-seed", 0);
            dropI.setValue(1);
            save(comp, "codec" + codec + "-all-i-drop", 5);
            dropI.setValue(0);
            codecParam(fx, "GOP Length").setValue(6);
            if (codec !== 2) {
                codecParam(fx, "Allow Future Frames").setValue(1);
                save(comp, "codec" + codec + "-b-normal", 2);
                dropB.setValueAtTime(0, 0);
                dropB.setValueAtTime(1 / 24, 1);
                dropB.setValueAtTime(3 / 24, 0);
                save(comp, "codec" + codec + "-b-drop", 2);
                app.purge(PurgeTarget.ALL_CACHES);
                save(comp, "codec" + codec + "-b-replay", 2);
                save(comp, "codec" + codec + "-b-reset", 3);
                while (dropB.numKeys) { dropB.removeKey(dropB.numKeys); }
                dropB.setValue(0);
                save(comp, "codec" + codec + "-b-normal3", 3);
                codecParam(fx, "Allow Future Frames").setValue(0);
            }
            codecParam(fx, "Map Source").setValue(2);
            codecParam(fx, "Output").setValue(1);
            save(comp, "codec" + codec + "-isolated", 8);
            fx.enabled = false; save(comp, "codec" + codec + "-reference", 8); fx.enabled = true;
            codecParam(fx, "Map Source").setValue(1);
            codecParam(fx, "Output").setValue(2);
        }
        codecParam(fx, "Codec").setValue(1);
        dropI.setValueAtTime(0, 0);
        dropI.setValueAtTime(6 / 24, 1);
        dropI.setValueAtTime(12 / 24, 0);
        comp.time = 8 / 24;
        comp.openInViewer(); layer.selected = true;
        app.project.save(new File(root.fsName + "/codecs.aep"));
        note("Completed. Check all PNGs externally, including RGB hold (alpha stays current).");
    } catch (e) { note("FAIL " + e.toString() + " line " + e.line); throw e; }
}());
