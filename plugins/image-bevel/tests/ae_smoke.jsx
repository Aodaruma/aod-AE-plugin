// Run in a fresh, dedicated AE instance. Outputs stay inside this checkout's target directory.
(function () {
    app.exitAfterLaunchAndEval = true;
    var root = File($.fileName).parent.parent.parent.parent;
    var out = new Folder(root.fsName + "/target/image-bevel-smoke"); out.create();
    var report = new File(out.fsName + "/report.txt");
    report.encoding = "UTF-8";
    report.open("w"); report.close();
    function log(s) { report.open("a"); report.writeln(s); report.close(); }
    function find(group, name) {
        for (var i = 1; i <= group.numProperties; i++) {
            var p = group.property(i);
            if (p.name === name) return p;
            if (p.numProperties) { var found = find(p, name); if (found) return found; }
        }
        return null;
    }
    function set(effect, name, value) {
        var p = find(effect, name); if (!p) throw Error("Missing parameter: " + name);
        p.setValue(value);
    }
    function rect(layer, name, vertices, mode) {
        var m = layer.property("ADBE Mask Parade").addProperty("ADBE Mask Atom"); m.name = name;
        var s = new Shape(); s.vertices = vertices; s.closed = true;
        s.inTangents = [[0,0],[0,0],[0,0],[0,0]]; s.outTangents = s.inTangents;
        m.property("ADBE Mask Shape").setValue(s); m.maskMode = mode;
    }
    try {
        if (app.project.numItems !== 0) throw Error("Use a fresh AE instance; current project is preserved.");
        log("AE " + app.version);
        log("Initial working space: " + app.project.workingSpace);
        app.project.workingSpace = "";
        app.project.linearizeWorkingSpace = false;
        var source = app.project.items.addComp("Bevel Test Source", 256, 256, 1, 1, 24);
        var solid = source.layers.addSolid([1,1,1], "Silhouette", 256, 256, 1);
        rect(solid, "Rectangle", [[64,64],[192,64],[192,192],[64,192]], MaskMode.ADD);
        var bg = app.project.items.addComp("Refraction Grid", 256, 256, 1, 1, 24);
        bg.layers.addSolid([0.08,0.15,0.25], "Dark", 256,256,1);
        for (var gx = 0; gx < 8; gx++) for (var gy = 0; gy < 8; gy++) {
            if ((gx + gy) % 2 === 0) {
                var tile = bg.layers.addSolid([0.9,0.5,0.12], "Tile", 32,32,1);
                tile.property("ADBE Transform Group").property("ADBE Position").setValue([gx*32+16,gy*32+16]);
            }
        }
        function fixture(name, depth, changes, refract) {
            app.project.bitsPerChannel = depth;
            var comp = app.project.items.addComp(name,256,256,1,1,24);
            var backdrop = refract ? comp.layers.add(bg) : null;
            if (backdrop) backdrop.enabled = false;
            var layer = comp.layers.add(source);
            var effect = layer.property("ADBE Effect Parade").addProperty("ImageBevel");
            set(effect, "Bevel Width (px)", 24);
            for (var i = 0; i < changes.length; i++) set(effect, changes[i][0], changes[i][1]);
            if (backdrop) set(effect, "Refraction Layer", backdrop.index);
            var viewer = comp.openInViewer();
            if (viewer) viewer.views[0].options.exposure = 0;
            // Query float pixels directly: AE 25 saveFrameToPng remaps HDR values.
            var probe = comp.layers.addText("");
            var value = probe.property("ADBE Text Properties").property("ADBE Text Document");
            value.expression = 'var a=[];for(var x=0;x<256;x++)a.push(thisComp.layer(2).sampleImage([x+0.5,128.5],[0.5,0.5],true,0).join(","));a.join("\\n")';
            var samples = new File(out.fsName + "/" + name + ".csv");
            samples.encoding = "UTF-8"; samples.open("w"); samples.write(value.value.text); samples.close();
            if (value.expressionError) throw Error(value.expressionError);
            probe.remove();
            comp.saveFrameToPng(0, new File(out.fsName + "/" + name + ".png"));
            log("PASS " + name);
            return comp;
        }
        for (var b = 0; b < 3; b++) {
            var depth = [8,16,32][b];
            fixture("lit-" + depth, depth, [], false);
            fixture("height-" + depth, depth, [["Output",2],["Alpha",3]], false);
        }
        fixture("alpha-original",32,[["Output",2],["Direction",2]],false);
        fixture("alpha-curve",32,[["Output",2],["Alpha",2],["Direction",2]],false);
        fixture("alpha-none",32,[["Output",2],["Alpha",3],["Direction",2]],false);
        fixture("direction-both",32,[["Output",2],["Alpha",3],["Direction",3]],false);
        fixture("offset",32,[["Output",2],["Alpha",3],["Contour Offset (px)",12]],false);
        fixture("neumann",32,[["Output",2],["Surface Method",2]],false);
        fixture("dirichlet",32,[["Output",2],["Surface Method",3]],false);
        fixture("normal",32,[["Output",3]],false);
        fixture("blur",32,[["Output",2],["Silhouette Blur (px)",2],["Height Blur (px)",2]],false);
        fixture("emission",32,[["Light Intensity",0],["Ambient",0],["Emission Strength",0.5],["Emission Color",[1,0.2,0,1]]],false);
        fixture("refraction-ior1",32,[["Transmission",1],["Output",6],["IOR",1],["Base Color",[1,1,1,1]],["Refraction Depth (px)",64]],true);
        fixture("refraction-ior15",32,[["Transmission",1],["Output",6],["IOR",1.5],["Base Color",[1,1,1,1]],["Refraction Depth (px)",64]],true);
        fixture("emission-hdr",32,[["Light Intensity",0],["Ambient",0],["Emission Strength",3],["Emission Color",[1,0.2,0,1]]],false);
        app.project.bitsPerChannel = 16;
        var demoSource = app.project.items.addComp("Bevel Demo Source",512,512,1,1,24);
        var text = demoSource.layers.addText("AOD");
        var document = text.property("ADBE Text Properties").property("ADBE Text Document").value;
        document.font = "Arial-BoldMT"; document.fontSize = 156; document.fillColor = [1,1,1];
        document.justification = ParagraphJustification.CENTER_JUSTIFY;
        text.property("ADBE Text Properties").property("ADBE Text Document").setValue(document);
        text.property("ADBE Transform Group").property("ADBE Position").setValue([256,310]);
        var demo = app.project.items.addComp("AOD Image Bevel Demo",512,512,1,1,24);
        demo.layers.addSolid([0.025,0.035,0.055], "Background",512,512,1);
        var demoLayer = demo.layers.add(demoSource);
        var bevel = demoLayer.property("ADBE Effect Parade").addProperty("ImageBevel");
        set(bevel,"Bevel Width (px)",12); set(bevel,"Height (px)",10); set(bevel,"Height Blur (px)",1.2);
        set(bevel,"Base Color",[0.2,0.55,0.9,1]); set(bevel,"Roughness",0.36);
        set(bevel,"Metallic",0.3); set(bevel,"Ambient",0.3); set(bevel,"Light Position",[145,70]);
        set(bevel,"Light Height (px)",300); set(bevel,"Light Intensity",4);
        demo.saveFrameToPng(0,new File(out.fsName + "/demo.png"));
        demo.openInViewer(); demoLayer.selected = true; bevel.selected = true;
        app.project.save(new File(out.fsName + "/image-bevel-smoke.aep"));
        log("COMPLETE");
    } catch (e) { log("ERROR " + e.toString() + " line " + e.line); }
}());
