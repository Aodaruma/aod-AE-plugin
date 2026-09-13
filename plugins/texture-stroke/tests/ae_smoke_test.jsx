// Run with AfterFX -r. Creates a temporary project folder and removes it afterwards.
(function () {
    var out = new Folder($.global.TEXTURE_STROKE_TEST_OUTPUT || (Folder.temp.fsName + "/texture-stroke-smoke"));
    if (!out.exists && !out.create()) { throw new Error("Cannot create " + out.fsName); }
    var log = new File(out.fsName + "/result.txt");
    if (!log.open("w")) { throw new Error("Cannot write " + log.fsName); }
    function record(s) { log.writeln(s); log.close(); log.open("a"); }
    var folder = app.project.items.addFolder("TextureStroke regression fixtures");
    var bpc = app.project.bitsPerChannel;
    app.beginSuppressDialogs();
    try {
        function comp(name) {
            var c = app.project.items.addComp(name, 320, 240, 1, 1, 24);
            c.parentFolder = folder;
            return c;
        }
        function effect(layer) {
            var e = layer.property("ADBE Effect Parade").addProperty("TextureStroke");
            e.property("Output").setValue(2);
            e.property("Stroke Width (px)").setValue(16);
            return e;
        }
        function save(c, name, time) {
            var file = new File(out.fsName + "/" + name + ".png");
            if (file.exists) { file.remove(); }
            c.saveFrameToPng(time || 0, file);
            var deadline = new Date().getTime() + 10000;
            while ((!file.exists || file.length === 0) && new Date().getTime() < deadline) {
                $.sleep(25);
                file = new File(file.fsName);
            }
            if (!file.exists || file.length === 0) { throw new Error("PNG was not saved: " + name); }
            record("rendered " + name);
        }
        function outlinePair(name, vertices, incoming, outgoing, closed) {
            var c = comp(name);
            var l = c.layers.addShape();
            var contents = l.property("ADBE Root Vectors Group");
            var g = contents.addProperty("ADBE Vector Group");
            var vectors = g.property("ADBE Vectors Group");
            var bp = vectors.addProperty("ADBE Vector Shape - Group");
            var path = new Shape();
            path.vertices = vertices;
            path.inTangents = incoming;
            path.outTangents = outgoing;
            path.closed = closed;
            bp.property("ADBE Vector Shape").setValue(path);
            var se = effect(l);
            se.property("Stroke Width (px)").setValue(2);
            se.property("Fallback Brush Softness (%)").setValue(0);
            save(c, name);
            se.enabled = false;
            var stroke = vectors.addProperty("ADBE Vector Graphic - Stroke");
            stroke.property("ADBE Vector Stroke Color").setValue([1,1,1]);
            stroke.property("ADBE Vector Stroke Width").setValue(2);
            stroke.property("ADBE Vector Stroke Line Cap").setValue(2);
            stroke.property("ADBE Vector Stroke Line Join").setValue(2);
            save(c, name + "-nativeStroke");
        }
        // The raw outline conversion must preserve the entire contour, not
        // merely its bounds. Zero handles exposed the inward-bending bug.
        outlinePair("shape-polygon", [[-95,-65],[65,-85],[115,20],[70,85],[-100,65]],
            [[0,0],[0,0],[0,0],[0,0],[0,0]], [[0,0],[0,0],[0,0],[0,0],[0,0]], true);
        outlinePair("shape-asymmetric-curve", [[-100,-50],[80,70]],
            [[0,0],[-25,-100]], [[75,-45],[0,0]], false);
        outlinePair("shape-translated-curve", [[-65,-30],[115,90]],
            [[0,0],[-25,-100]], [[75,-45],[0,0]], false);
        var c = comp("Masks");
        var l = c.layers.addSolid([0, 0, 0], "Masks", 320, 240, 1);
        l.source.parentFolder = folder;
        var mask = l.Masks.addProperty("ADBE Mask Atom");
        var path = new Shape();
        path.vertices = [[70, 60], [250, 60], [250, 180], [70, 180]];
        path.inTangents = [[0,0], [0,0], [0,0], [0,0]];
        path.outTangents = [[0,0], [0,0], [0,0], [0,0]];
        path.closed = true;
        mask.maskShape.setValue(path);
        var e = effect(l);
        for (var bi = 0; bi < 3; bi++) {
            app.project.bitsPerChannel = [8,16,32][bi];
            mask.maskMode = MaskMode.NONE;
            save(c, "mask-none-" + app.project.bitsPerChannel);
            mask.maskMode = MaskMode.ADD;
            save(c, "mask-add-" + app.project.bitsPerChannel);
        }
        c.resolutionFactor = [2,2];
        save(c, "mask-half");
        c.resolutionFactor = [1,1];
        e.property("Stroke Width (px)").setValue(0);
        save(c, "zero-width");
        e.property("Stroke Width Source").setValue(2);
        mask.maskFeather.setValue([20,20]);
        save(c, "mask-feather-width");
        c.resolutionFactor = [2,2];
        save(c, "mask-feather-half");
        c = comp("Shape rectangle");
        l = c.layers.addShape();
        var g = l.property("ADBE Root Vectors Group").addProperty("ADBE Vector Group");
        g.property("ADBE Vectors Group").addProperty("ADBE Vector Shape - Rect").property("ADBE Vector Rect Size").setValue([180,120]);
        effect(l);
        save(c, "shape-rect");
        var se = l.property("ADBE Effect Parade").property(1);
        se.enabled = false;
        var nativeStroke = g.property("ADBE Vectors Group").addProperty("ADBE Vector Graphic - Stroke");
        nativeStroke.property("ADBE Vector Stroke Color").setValue([1,1,1]);
        nativeStroke.property("ADBE Vector Stroke Width").setValue(16);
        save(c, "shape-rect-nativeStroke");
        nativeStroke.enabled = false;
        se.enabled = true;
        l.property("ADBE Transform Group").property("ADBE Position").setValue([175,135]);
        l.property("ADBE Transform Group").property("ADBE Rotate Z").setValue(20);
        g.property("ADBE Vector Transform Group").property("ADBE Vector Position").setValue([10,-5]);
        g.property("ADBE Vector Transform Group").property("ADBE Vector Scale").setValue([80,120]);
        g.property("ADBE Vector Transform Group").property("ADBE Vector Skew").setValue(15);
        g.property("ADBE Vector Transform Group").property("ADBE Vector Skew Axis").setValue(30);
        save(c, "shape-transform");
        se.enabled = false;
        nativeStroke.enabled = true;
        save(c, "shape-transform-nativeStroke");
        se.property("Stroke Width (px)").setValue(2);
        se.enabled = true;
        nativeStroke.enabled = false;
        save(c, "shape-transform-thin");
        se.enabled = false;
        nativeStroke.enabled = true;
        nativeStroke.property("ADBE Vector Stroke Width").setValue(2);
        save(c, "shape-transform-thin-nativeStroke");
        nativeStroke.enabled = false;
        se.enabled = true;
        g.enabled = false;
        save(c, "shape-disabled");
        c = comp("Bezier shape");
        l = c.layers.addShape();
        g = l.property("ADBE Root Vectors Group").addProperty("ADBE Vector Group");
        var bp = g.property("ADBE Vectors Group").addProperty("ADBE Vector Shape - Group");
        var curve = new Shape();
        curve.vertices = [[-90,0],[90,0]];
        curve.inTangents = [[0,0],[-30,80]];
        curve.outTangents = [[30,-80],[0,0]];
        curve.closed = false;
        bp.property("ADBE Vector Shape").setValue(curve);
        se = effect(l);
        save(c, "shape-bezier");
        se.enabled = false;
        nativeStroke = g.property("ADBE Vectors Group").addProperty("ADBE Vector Graphic - Stroke");
        nativeStroke.property("ADBE Vector Stroke Color").setValue([1,1,1]);
        nativeStroke.property("ADBE Vector Stroke Width").setValue(16);
        save(c, "shape-bezier-nativeStroke");
        c = comp("Ellipse shape");
        l = c.layers.addShape();
        g = l.property("ADBE Root Vectors Group").addProperty("ADBE Vector Group");
        g.property("ADBE Vectors Group").addProperty("ADBE Vector Shape - Ellipse").property("ADBE Vector Ellipse Size").setValue([180,120]);
        se = effect(l);
        save(c, "shape-ellipse");
        c.resolutionFactor = [2,2];
        save(c, "shape-ellipse-half");
        c.resolutionFactor = [1,1];
        se.enabled = false;
        nativeStroke = g.property("ADBE Vectors Group").addProperty("ADBE Vector Graphic - Stroke");
        nativeStroke.property("ADBE Vector Stroke Color").setValue([1,1,1]);
        nativeStroke.property("ADBE Vector Stroke Width").setValue(16);
        save(c, "shape-ellipse-nativeStroke");
        // Output must composite the source only once, after the entire stroke
        // when Behind is selected. A cropped green solid exposes both sides.
        app.project.bitsPerChannel = 8;
        c = comp("Composite order");
        l = c.layers.addSolid([0, 1, 0], "Green source", 320, 240, 1);
        l.source.parentFolder = folder;
        mask = l.Masks.addProperty("ADBE Mask Atom");
        mask.maskShape.setValue(path);
        mask.maskMode = MaskMode.ADD;
        e = effect(l);
        e.property("Fallback Brush Softness (%)").setValue(0);
        e.property("Output").setValue(1);
        save(c, "composite-front");
        e.property("Output").setValue(3);
        save(c, "composite-behind");

        // Animated red/blue brush on a self-crossing path makes the order
        // at the intersection unambiguous while testing texture checkout.
        var texture = comp("Animated brush");
        var red = texture.layers.addSolid([1,0,0], "Red", 320, 240, 1);
        red.source.parentFolder = folder;
        red.outPoint = 0.5;
        var blue = texture.layers.addSolid([0,0,1], "Blue", 320, 240, 1);
        blue.source.parentFolder = folder;
        blue.inPoint = 0.5;
        c = comp("Stamp order");
        var textureLayer = c.layers.add(texture);
        textureLayer.enabled = false;
        l = c.layers.addShape();
        g = l.property("ADBE Root Vectors Group").addProperty("ADBE Vector Group");
        bp = g.property("ADBE Vectors Group").addProperty("ADBE Vector Shape - Group");
        curve = new Shape();
        curve.vertices = [[-90,-70],[90,70],[-90,70],[90,-70]];
        curve.inTangents = [[0,0],[0,0],[0,0],[0,0]];
        curve.outTangents = [[0,0],[0,0],[0,0],[0,0]];
        curve.closed = false;
        bp.property("ADBE Vector Shape").setValue(curve);
        se = effect(l);
        se.property("Texture Layer").setValue(textureLayer.index);
        // The topic and popup share a display name; address the saved popup ID.
        var timeMode = se.property("TextureStroke-424488831");
        timeMode.setValue(3);
        se.property("Time Range (frames)").setValue(12);
        se.property("Time Samples").setValue(2);
        save(c, "stamp-forward", 0.5);
        se.property("Stamp Order").setValue(2);
        save(c, "stamp-reverse", 0.5);
        se.property("Stamp Order").setValue(1);
        timeMode.setValue(1);
        save(c, "time-current-red", 0.25);
        save(c, "time-current-blue", 0.75);
        timeMode.setValue(4);
        save(c, "time-random", 0.5);
        record("PASS");
    } catch (err) {
        record("FAIL line " + err.line + ": " + err.toString());
        throw err;
    } finally {
        app.project.bitsPerChannel = bpc;
        folder.remove();
        app.endSuppressDialogs(false);
        log.close();
    }
})();
