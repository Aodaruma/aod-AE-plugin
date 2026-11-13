// AE 2025 host fixtures. Set TEXTURE_STROKE_TEST_OUTPUT to a task directory.
(function () {
    var out = new Folder($.global.TEXTURE_STROKE_TEST_OUTPUT);
    if (!$.global.TEXTURE_STROKE_TEST_OUTPUT || (!out.exists && !out.create())) throw Error("Set test output directory");
    var folder = app.project.items.addFolder("TextureStroke dynamics fixtures");
    var originalBpc = app.project.bitsPerChannel;
    var log = new File(out.fsName + "/result.txt"); log.open("w");
    function record(s) { log.writeln(s); log.close(); log.open("a"); }
    function comp(name,w,h,d,fps) { var c=app.project.items.addComp(name,w,h,1,d||8,fps||24);c.parentFolder=folder;return c; }
    function solid(c,color,name,w,h) {var l=c.layers.addSolid(color,name,w||c.width,h||c.height,1);l.source.parentFolder=folder;return l;}
    function line(c,vertices,name) {
        var l=c.layers.addShape();l.name=name||"Stroke";l.transform.position.setValue([0,0]);
        var group=l.property("ADBE Root Vectors Group").addProperty("ADBE Vector Group");
        var bp=group.property("ADBE Vectors Group").addProperty("ADBE Vector Shape - Group");
        var s=new Shape();s.vertices=vertices;s.inTangents=[];s.outTangents=[];s.closed=false;
        for(var i=0;i<vertices.length;i++){s.inTangents.push([0,0]);s.outTangents.push([0,0]);}
        bp.property("ADBE Vector Shape").setValue(s);
        var e=l.property("ADBE Effect Parade").addProperty("TextureStroke");
        e.property("Output").setValue(2);e.property("Stroke Width (px)").setValue(12);
        e.property("Fallback Brush Softness (%)").setValue(0);
        return {layer:l,effect:e,path:bp.property("ADBE Vector Shape")};
    }
    function save(c,name,t) {
        var f=new File(out.fsName+"/"+name+".png");if(f.exists)f.remove();c.saveFrameToPng(t||0,f);
        var until=new Date().getTime()+10000;
        while((!f.exists||!f.length)&&new Date().getTime()<until){$.sleep(25);f=new File(f.fsName);}
        if(!f.exists||!f.length)throw Error("Missing "+name);record("rendered "+name);
    }
    app.beginSuppressDialogs();
    try {
        app.project.bitsPerChannel=8;
        var brush=comp("Four frame brush",24,12,4/12,12);
        var colors=[[1,0,0],[0,1,0],[0,0,1],[1,1,0]];
        for(var i=0;i<4;i++){var b=solid(brush,colors[i],"Frame "+i);b.inPoint=i/12;b.outPoint=(i+1)/12;}
        var c=comp("Random Still",320,240);
        var texture=c.layers.add(brush);texture.startTime=2;texture.enabled=false;
        var stroke=line(c,[[30,120],[290,120]]);var e=stroke.effect;
        e.property("Texture Layer").setValue(texture.index);
        e.property("TextureStroke-424488831").setValue(4);
        e.property("Time Samples").setValue(16);
        e.property("Time Range (frames)").setValue(100000);
        save(c,"random-early",0);save(c,"random-late",6);
        e.property("Random Time Seed").setValue(33);save(c,"random-seed",6);
        e.property("Random Time Seed").setValue(0);
        texture.inPoint=2+1/12;texture.outPoint=2+3/12;
        save(c,"random-trimmed",6);
        texture.stretch=200;texture.startTime=2;texture.inPoint=2;texture.outPoint=2+8/12;
        save(c,"random-stretched",6);
        // AE reverses the in/out ordering for negative stretch. Let AE create
        // that interval rather than assigning ascending in/out points.
        var reverse=c.layers.add(brush);reverse.enabled=false;reverse.startTime=2;reverse.stretch=-100;
        e.property("Texture Layer").setValue(reverse.index);
        save(c,"random-reverse",6);
        reverse.inPoint=reverse.outPoint+0.01;save(c,"random-reverse-short",6);
        e.property("Texture Layer").setValue(texture.index);
        stroke.layer.startTime=1;stroke.layer.stretch=150;
        save(c,"random-owner-early",2);save(c,"random-owner-late",6);

        var mapComp=comp("Map stripes",160,120);
        for(i=0;i<3;i++){
            var m=solid(mapComp,[(i+1)/4,(i+1)/4,(i+1)/4],"Stripe "+i,54,120);
            m.transform.position.setValue([(i+0.5)*160/3,60]);
        }
        c=comp("Map Coordinates",320,240);
        var map=c.layers.add(mapComp);map.enabled=false;
        stroke=line(c,[[30,120],[290,120]]);e=stroke.effect;
        e.property("Size Input").setValue(5);e.property("Size Map Layer").setValue(map.index);
        save(c,"map-size",0);
        map.transform.position.setValue([200,120]);save(c,"map-size-shift",0);
        map.transform.position.setValue([160,120]);map.transform.scale.setValue([150,100]);save(c,"map-size-scale",0);
        map.transform.scale.setValue([100,100]);c.resolutionFactor=[2,2];save(c,"map-size-half",0);c.resolutionFactor=[1,1];
        e.property("Size Input").setValue(1);
        e.property("Opacity Input").setValue(5);e.property("Opacity Map Layer").setValue(map.index);
        save(c,"map-opacity",0);
        e.property("Opacity Input").setValue(1);
        e.property("Rotation Input").setValue(5);e.property("Rotation Map Layer").setValue(map.index);
        e.property("Texture Layer").setValue(c.layers.add(brush).index);
        c.layer(1).enabled=false;e.property("TextureStroke-424488831").setValue(2);e.property("Fixed Frame").setValue(0);
        save(c,"map-rotation",0);
        e.property("Rotation Input").setValue(1);e.property("Texture Layer").setValue(0);
        e.property("Density Input").setValue(5);e.property("Density Map Layer").setValue(map.index);
        e.property("Density (%)").setValue(10);save(c,"map-density",0);

        // Alpha maps, parent transforms, and an animated map must update the
        // render without purging AE's frame cache.
        e.property("Density Input").setValue(1);e.property("Density (%)").setValue(100);
        e.property("Size Input").setValue(6);e.property("Size Map Layer").setValue(map.index);
        for(i=1;i<=3;i++){mapComp.layer(i).transform.opacity.setValue((4-i)*25);}
        save(c,"map-alpha",0);
        var parent=c.layers.addNull();parent.source.parentFolder=folder;map.parent=parent;
        var parentPos=parent.transform.position.value;parent.transform.position.setValue([parentPos[0]+40,parentPos[1]]);
        save(c,"map-parent",0);
        map.parent=null;map.transform.position.setValue([160,120]);
        for(i=1;i<=3;i++){mapComp.layer(i).transform.opacity.setValue(100);}

        c=comp("Mask Map Coordinates",320,240);
        map=c.layers.add(mapComp);map.enabled=false;
        var owner=solid(c,[1,1,1],"Masked owner",240,180);
        var mask=owner.Masks.addProperty("ADBE Mask Atom");
        var maskPath=new Shape();maskPath.vertices=[[20,90],[220,90],[220,110],[20,110]];
        maskPath.inTangents=[[0,0],[0,0],[0,0],[0,0]];maskPath.outTangents=maskPath.inTangents;maskPath.closed=true;
        mask.maskShape.setValue(maskPath);mask.maskMode=MaskMode.NONE;
        e=owner.property("ADBE Effect Parade").addProperty("TextureStroke");e.property("Output").setValue(2);
        e.property("Stroke Width (px)").setValue(12);e.property("Fallback Brush Softness (%)").setValue(0);
        e.property("Size Input").setValue(5);e.property("Size Map Layer").setValue(map.index);
        save(c,"map-mask-none",0);mask.maskMode=MaskMode.ADD;save(c,"map-mask-cropped",0);
        c.resolutionFactor=[2,2];save(c,"map-mask-half",0);c.resolutionFactor=[1,1];

        c=comp("Path Metrics",320,240);stroke=line(c,[[30,90],[290,90]]);e=stroke.effect;
        e.property("Size Input").setValue(4);e.property("Length Reference (px)").setValue(520);save(c,"length-neutral",0);
        e.property("Length Reference (px)").setValue(260);save(c,"length-double",0);
        e.property("Size Input").setValue(3);e.property("Crowding Radius (px)").setValue(40);
        var root=stroke.layer.property("ADBE Root Vectors Group");
        var g=root.addProperty("ADBE Vector Group");var path=g.property("ADBE Vectors Group").addProperty("ADBE Vector Shape - Group");
        var s=new Shape();s.vertices=[[160,100],[290,100]];s.inTangents=[[0,0],[0,0]];s.outTangents=[[0,0],[0,0]];s.closed=false;
        path.property("ADBE Vector Shape").setValue(s);save(c,"crowding",0);
        e.property("Size Input").setValue(2);
        var originalPath=root.property(1).property("ADBE Vectors Group").property(1).property("ADBE Vector Shape");
        var p=originalPath.value;p.outTangents=[[50,100],[0,0]];p.inTangents=[[0,0],[-50,100]];originalPath.setValue(p);save(c,"curvature",0);
        e.property("Size Input").setValue(4);e.property("Length Reference (px)").setValue(520);
        c.openInViewer();stroke.layer.selected=true;
        app.project.save(new File(out.fsName+"/dynamics.aep"));
        record("PASS");
    } catch(err){record("FAIL line "+err.line+": "+err.toString());throw err;}
    finally{app.project.bitsPerChannel=originalBpc;app.endSuppressDialogs(false);log.close();}
    return "Dynamics fixtures saved";
}());
