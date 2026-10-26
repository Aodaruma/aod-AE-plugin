// Run in AE with AfterFX -r; all output stays in this checkout's target directory.
(function () {
    var root = File($.fileName).parent.parent.parent.parent;
    var out = new Folder(root.fsName + "/target/curve-smoke");
    if (!out.exists && !out.create()) { throw new Error("Cannot create output folder"); }
    var log = new File(out.fsName + "/result.txt");
    log.open("w");
    function record(s) { log.writeln(s); log.close(); log.open("a"); }
    var folder = app.project.items.addFolder("SpaceConvert CurveAlonged verification");
    var oldBpc = app.project.bitsPerChannel;
    app.beginSuppressDialogs();
    app.beginUndoGroup("Verify CurveAlonged");
    function comp(name, par) {
        var c = app.project.items.addComp(name, 320, 240, par || 1, 2, 24);
        c.parentFolder = folder;
        return c;
    }
    function save(c, name, time) {
        var f = new File(out.fsName + "/" + name + ".png");
        c.saveFrameToPng(time || 0, f);
        record("rendered " + name);
    }
    function shape(vertices, incoming, outgoing, closed) {
        var s = new Shape(); s.vertices = vertices; s.inTangents = incoming;
        s.outTangents = outgoing; s.closed = closed; return s;
    }
    function line(y) { return shape([[0,y],[319,y]],[[0,0],[0,0]],[[0,0],[0,0]],false); }
    function circle() {
        var k = 38.659932;
        return shape([[230,120],[160,190],[90,120],[160,50]],
            [[0,-k],[k,0],[0,k],[-k,0]], [[0,k],[-k,0],[0,-k],[k,0]],true);
    }
    function mask(layer, path) {
        var m = layer.Masks.addProperty("ADBE Mask Atom");
        m.maskMode = MaskMode.NONE; m.maskShape.setValue(path); return m;
    }
    function effect(layer, direction) {
        var e = layer.property("ADBE Effect Parade").addProperty("SpaceConvert");
        e.property("Space").setValue(10); e.property("Direction").setValue(direction || 1);
        e.property("Mask Path").setValue(1); e.property("Band Width").setValue(239);
        e.property("Interpolation").setValue(2); e.property("Edge").setValue(1);
        return e;
    }
    try {
        record("After Effects " + app.version);
        var pattern = comp("Curve test pattern");
        for (var y = 0; y < 6; y++) { for (var x = 0; x < 8; x++) {
            var tile = pattern.layers.addSolid([(x+1)/8,(y+1)/6,((x+y)%2)*0.65+0.15],"Tile",40,40,1,2);
            tile.position.setValue([x*40+20,y*40+20]); tile.source.parentFolder = folder;
        }}
        save(pattern,"source");
        var c = comp("Straight path"); var layer = c.layers.add(pattern);
        var m = mask(layer,line(119.5)); var e = effect(layer);
        record("properties " + e.numProperties + "; CurveAlonged=" + e.property("Space").value);
        for (var i = 0; i < 3; i++) {
            app.project.bitsPerChannel = [8,16,32][i];
            save(pattern,"source-"+[8,16,32][i]);
            e.property("Direction").setValue(1); save(c,"straight-forward-"+[8,16,32][i]);
            e.property("Direction").setValue(2); save(c,"straight-inverse-"+[8,16,32][i]);
        }
        app.project.bitsPerChannel = 8;
        e.property("Direction").setValue(1);
        c.resolutionFactor = [2,2]; save(c,"straight-half");
        c.resolutionFactor = [2,4]; save(c,"straight-anisotropic");
        c.resolutionFactor = [1,1];
        e.property("Mask Path").setValue(0); save(c,"missing-path");
        e.property("Mask Path").setValue(1);
        m.maskShape.setValueAtTime(0,line(70)); m.maskShape.setValueAtTime(1,line(170));
        e.property("Band Width").setValue(80);
        save(c,"animated-0",0); save(c,"animated-1",1);
        var ring = comp("Closed path"); var rl = ring.layers.add(pattern);
        mask(rl,circle()); var re = effect(rl,2); re.property("Band Width").setValue(40);
        save(ring,"ring-inverse");
        re.property("Direction").setValue(1); save(ring,"ring-forward");
        re.property("Along Offset (%)").setValue(25); save(ring,"ring-loop");
        re.property("Loop Closed Path").setValue(0); save(ring,"ring-no-loop");
        re.property("Loop Closed Path").setValue(1); re.property("Along Offset (%)").setValue(0);
        re.property("Fit").setValue(2); save(ring,"ring-contain");
        re.property("Fit").setValue(3); save(ring,"ring-cover");
        re.property("Fit").setValue(1); re.property("Direction").setValue(2);
        re.property("Reverse Along").setValue(1); re.property("Flip Normal").setValue(1); save(ring,"ring-flipped");
        re.property("Reverse Along").setValue(0); re.property("Flip Normal").setValue(0);
        re.property("Position").setValue([180,120]); save(ring,"ring-post-transform");
        var scomp = comp("Bezier path"); var sl = scomp.layers.add(pattern);
        mask(sl,shape([[35,180],[160,120],[285,60]],[[0,0],[-70,-75],[-45,0]],[[45,0],[70,75],[0,0]],false));
        var se = effect(sl,2); se.property("Band Width").setValue(55); save(scomp,"bezier-inverse");
        se.property("Direction").setValue(1); save(scomp,"bezier-forward");
        var parPattern = pattern.duplicate(); parPattern.pixelAspect = 2;
        var par = comp("Non-square pixels",2); var pl = par.layers.add(parPattern);
        mask(pl,line(119.5)); var pe = effect(pl,1);
        pe.enabled = false; save(par,"par-source"); pe.enabled = true;
        save(par,"par-fill"); pe.property("Fit").setValue(2); save(par,"par-contain");
        // Save only the fixtures created by this run in a separate reproducible project.
        if ($.global.CURVE_SMOKE_SAVE_PROJECT) { app.project.save(new File(out.fsName+"/curve-smoke.aep")); }
        record("PASS");
    } catch (err) { record("FAIL " + err.toString() + " line " + err.line); }
    finally {
        app.project.bitsPerChannel = oldBpc;
        app.endUndoGroup(); app.endSuppressDialogs(false); log.close();
    }
}());
