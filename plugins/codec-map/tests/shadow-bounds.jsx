// SPDX-License-Identifier: MPL-2.0
// Run after replay-bounds.jsx in its validation project.
(function () {
    var root = new Folder(File($.fileName).parent.parent.parent.parent.fsName + "/target/codec-map-replay-bounds");
    var log = new File(root.fsName + "/shadow-report.txt");
    function note(s) { log.open("a"); log.writeln(s); log.close(); }
    function save(comp, name, frame) {
        comp.time = frame / 24;
        comp.saveFrameToPng(comp.time, new File(root.fsName + "/" + name + ".png"));
        note("requested " + name);
    }
    log.open("w"); log.close();
    var comp = app.project.items.addComp("Animated shadow bounds", 400, 300, 1, 2, 24);
    var layer = comp.layers.addSolid([0.6, 0.25, 0.8], "Shadow input", 129, 97, 1, 2);
    var effects = layer.property("ADBE Effect Parade");
    effects.addProperty("ADBE Drop Shadow");
    var fx = effects.addProperty("CodecMap");
    var shadow = effects.property(1);
    shadow.property(1).setValue([0, 0, 0, 1]);
    shadow.property(2).setValue(100);
    shadow.property(3).expression = "time*24*60";
    shadow.property(5).setValue(0);
    fx.property("GOP Length").setValue(1);
    fx.property("Output").setValue(2);
    for (var codec = 1; codec <= 3; codec++) {
        fx.property("Codec").setValue(codec);
        for (var direction = 0; direction < 2; direction++) {
            // Nonzero shadow alpha forces the upstream result bounds to change.
            // Straight RGB is constant: colored source, black everywhere else.
            shadow.property(4).expression = direction === 0 ? "2 + time*24*8" : "42 - time*24*8";
            var prefix = "shadow" + codec + (direction === 0 ? "-grow" : "-shrink");
            fx.property("Drop I-Frames").setValue(1);
            save(comp, prefix + "-drop", 5);
            app.purge(PurgeTarget.ALL_CACHES);
            save(comp, prefix + "-other", 2);
            save(comp, prefix + "-repeat", 5);
            fx.property("Drop I-Frames").setValue(0);
            save(comp, prefix + "-reference", 5);
        }
    }
    app.project.save(new File(root.fsName + "/replay-bounds.aep"));
    note("Completed. Validate all 55 PNG files externally.");
}());
