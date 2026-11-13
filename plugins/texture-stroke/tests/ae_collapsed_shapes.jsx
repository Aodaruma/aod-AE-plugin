// AE 2025 integration test. All outputs stay in the configured task directory.
(function () {
    var out = new Folder($.global.TEXTURE_STROKE_TEST_OUTPUT);
    if (!$.global.TEXTURE_STROKE_TEST_OUTPUT || (!out.exists && !out.create())) {
        throw new Error("Set TEXTURE_STROKE_TEST_OUTPUT to an output directory");
    }
    var folder = app.project.items.addFolder("TextureStroke collapsed regression");
    var originalBpc = app.project.bitsPerChannel;
    var log = new File(out.fsName + "/result.txt");
    log.open("w");
    function record(s) { log.writeln(s); log.close(); log.open("a"); }
    function comp(name, w, h) {
        var c = app.project.items.addComp(name, w, h, 1, 4, 24);
        c.parentFolder = folder;
        return c;
    }
    function save(c, name, time) {
        var file = new File(out.fsName + "/" + name + ".png");
        if (file.exists) { file.remove(); }
        c.saveFrameToPng(time || 0, file);
        var deadline = new Date().getTime() + 10000;
        while ((!file.exists || !file.length) && new Date().getTime() < deadline) {
            $.sleep(25); file = new File(file.fsName);
        }
        if (!file.exists || !file.length) { throw new Error("Missing render: " + name); }
        record("rendered " + name);
    }
    function curve(c, name) {
        var l = c.layers.addShape(); l.name = name;
        var g = l.property("ADBE Root Vectors Group").addProperty("ADBE Vector Group");
        var v = g.property("ADBE Vectors Group");
        var bp = v.addProperty("ADBE Vector Shape - Group");
        var s = new Shape();
        s.vertices = [[-65,-40],[55,45]];
        s.inTangents = [[0,0],[-25,-75]];
        s.outTangents = [[50,-25],[0,0]];
        s.closed = false;
        bp.property("ADBE Vector Shape").setValue(s);
        var stroke = v.addProperty("ADBE Vector Graphic - Stroke");
        stroke.property("ADBE Vector Stroke Color").setValue([1,1,1]);
        stroke.property("ADBE Vector Stroke Width").setValue(2);
        stroke.property("ADBE Vector Stroke Line Cap").setValue(2);
        return {layer:l, group:g, path:v.property(1).property("ADBE Vector Shape"), stroke:stroke};
    }
    function effect(l) {
        var e = l.property("ADBE Effect Parade").addProperty("TextureStroke");
        e.property("Output").setValue(2);
        e.property("Stroke Width (px)").setValue(2);
        e.property("Fallback Brush Softness (%)").setValue(0);
        return e;
    }
    function precomp(c, child, position, scale, rotation) {
        var l = c.layers.add(child);
        l.collapseTransformation = true;
        l.transform.position.setValue(position);
        l.transform.scale.setValue(scale);
        l.transform.rotation.setValue(rotation);
        return l;
    }
    function pair(c, e, name, time) {
        e.enabled = true; save(c, name, time);
        e.enabled = false; save(c, name + "-native", time);
        e.enabled = true;
    }
    app.beginSuppressDialogs();
    try {
        app.project.bitsPerChannel = 8;
        var leaf = comp("Leaf", 200, 160);
        var shape = curve(leaf, "Bezier");
        var c = comp("Collapsed", 480, 360);
        var pre = precomp(c, leaf, [245,175], [135,80], 23);
        pre.transform.anchorPoint.setValue([80,65]);
        var e = effect(pre);
        pair(c, e, "collapsed-transformed", 0);
        c.resolutionFactor = [2,2];
        pair(c, e, "collapsed-half", 0);
        c.resolutionFactor = [1,1];

        var mid = comp("Middle", 280, 220);
        precomp(mid, leaf, [145,120], [75,135], -19);
        var second = curve(mid, "Second independent shape");
        second.layer.transform.position.setValue([85,75]);
        second.layer.transform.scale.setValue([45,45]);
        c = comp("Nested", 480, 360);
        pre = precomp(c, mid, [240,185], [110,95], 14);
        var parent = c.layers.addNull(); parent.source.parentFolder = folder;
        parent.transform.position.setValue([255,165]);
        parent.transform.rotation.setValue(8);
        pre.parent = parent;
        e = effect(pre);
        pair(c, e, "collapsed-nested", 0);

        shape.layer.transform.position.setValueAtTime(0, [75,60]);
        shape.layer.transform.position.setValueAtTime(2, [130,105]);
        pre.startTime = 0.5;
        pre.stretch = 200;
        pair(c, e, "collapsed-stretched", 1.5);
        pre.timeRemapEnabled = true;
        pre.property("ADBE Time Remapping").setValueAtTime(0.5, 1.25);
        pre.property("ADBE Time Remapping").setValueAtTime(3.5, 1.25);
        pair(c, e, "collapsed-remapped", 1.5);

        var hidden = curve(leaf, "Hidden shape");
        hidden.layer.transform.position.setValue([20,25]);
        hidden.layer.enabled = false;
        var inactive = curve(leaf, "Out of range");
        inactive.layer.inPoint = 3;
        pair(c, e, "collapsed-visibility", 1.5);
        shape.layer.solo = true;
        pair(c, e, "collapsed-solo", 1.5);
        shape.layer.solo = false;

        c = comp("Adjustment", 480, 360);
        var below = curve(c, "Direct shape below adjustment");
        below.layer.transform.position.setValue([130,220]);
        precomp(c, leaf, [325,180], [95,105], -25);
        var adjustment = c.layers.addSolid([1,1,1], "Adjustment", 480, 360, 1);
        adjustment.source.parentFolder = folder;
        adjustment.adjustmentLayer = true;
        e = effect(adjustment);
        var above = curve(c, "Above adjustment - paths only");
        above.stroke.enabled = false;
        above.layer.transform.position.setValue([240,70]);
        pair(c, e, "adjustment-shapes", 1);
        c.resolutionFactor = [2,2];
        pair(c, e, "adjustment-half", 1);
        c.resolutionFactor = [1,1];

        // Geometry changes must invalidate the output even if the input pixels
        // stay transparent (no native shape fill/stroke and no cache purge).
        leaf = comp("Transparent source", 200, 160);
        shape = curve(leaf, "Invisible path styles");
        shape.stroke.enabled = false;
        c = comp("Geometry cache", 480, 360);
        pre = precomp(c, leaf, [240,180], [120,120], 0);
        e = effect(pre);
        save(c, "cache-before", 0);
        shape.layer.transform.position.setValue([145,105]);
        save(c, "cache-after", 0);
        shape.stroke.enabled = true;
        e.enabled = false;
        save(c, "cache-after-native", 0);
        record("PASS");
        if ($.global.TEXTURE_STROKE_KEEP_FIXTURES) {
            app.project.save(new File(out.fsName + "/collapsed-shapes.aep"));
        }
    } catch (err) {
        record("FAIL line " + err.line + ": " + err.toString());
        throw err;
    } finally {
        app.project.bitsPerChannel = originalBpc;
        if (!$.global.TEXTURE_STROKE_KEEP_FIXTURES) { folder.remove(); }
        app.endSuppressDialogs(false);
        log.close();
    }
}());
