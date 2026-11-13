// Run in a new isolated process: AfterFX.com -m -noui -r <this file>.
(function () {
    var root = File($.fileName).parent.parent.parent.parent;
    var folder = new Folder(root.fsName + "/target/panorama-smoke");
    folder.create();
    var report = new File(folder.fsName + "/report.txt");
    function log(s) { report.open("a"); report.writeln(s); report.close(); }
    if (app.project && (app.project.numItems > 0 || app.project.file)) { log("REFUSED: project is not empty."); return; }
    report.open("w"); report.close();
    function saveFrame(comp, destination) {
        if (destination.exists) { destination.remove(); }
        if (comp.resolutionFactor[0]===1) {
            comp.saveFrameToPng(0,destination);
            if (destination.exists && destination.length > 0) { return; }
        }
        // AE sometimes silently skips saveFrameToPng after project creation.
        var item=app.project.renderQueue.items.add(comp);
        if (comp.resolutionFactor[0]===2) { item.setSetting("Resolution","Half"); }
        item.timeSpanStart=0; item.timeSpanDuration=comp.frameDuration;
        var module=item.outputModule(1), template=null;
        for (var t=0;t<module.templates.length;t++) {
            if (/PNG/i.test(module.templates[t])) { template=module.templates[t]; break; }
        }
        if (!template) { throw new Error("PNG render template required"); }
        module.applyTemplate(template);
        var prefix="frame-"+new Date().getTime();
        module.file=new File(folder.fsName+"/"+prefix+"-[#####].png");
        app.project.renderQueue.render();
        var frames=folder.getFiles(prefix+"-*.png");
        if (item.status!==RQItemStatus.DONE || frames.length!==1 || !frames[0].copy(destination.fsName)) {
            throw new Error("Could not render "+comp.name);
        }
        frames[0].remove(); item.remove();
    }
    app.beginSuppressDialogs();
    try {
        log("AE " + app.version);
        var sources=[];
        for (var f=1;f<=5;f++) {
            var footage=app.project.importFile(new ImportOptions(new File(folder.fsName+"/input-"+f+".png")));
            footage.mainSource.alphaMode=AlphaMode.STRAIGHT;
            sources.push(footage);
        }
        function build(name, input, output, width, pan, reduced) {
            var c=app.project.items.addComp(name,640,480,1,1,24);
            if (reduced) { c.resolutionFactor=[2,2]; }
            var l=c.layers.add(sources[input-1]);
            var t=l.property("ADBE Transform Group");
            var before=[c.width,c.height,t.property("ADBE Position").value.toString(),t.property("ADBE Anchor Point").value.toString(),t.property("ADBE Scale").value.toString()].join("|");
            var e=l.property("ADBE Effect Parade").addProperty("PanoramaConvert");
            if(e.property("Operation") || e.property("Axis Point") || e.property("Axis Twist")) {
                throw new Error("Unexpected axis workflow controls; restart with version 0.3 or later.");
            }
            e.property("Input").setValue(input);
            e.property("Output").setValue(output);
            e.property("Output Width (0 = Input)").setValue(width);
            e.property("Pan (Y)").setValue(pan||0);
            var after=[c.width,c.height,t.property("ADBE Position").value.toString(),t.property("ADBE Anchor Point").value.toString(),t.property("ADBE Scale").value.toString()].join("|");
            if (before!==after) { throw new Error("Composition or transform changed: "+name); }
            return c;
        }
        var cases=[];
        for (var i=1;i<=5;i++) { for (var o=1;o<=5;o++) {
            cases.push(build("map-"+i+"-"+o,i,o,384,0,false));
        }}
        cases.push(build("identity",1,1,0,0,false));
        cases.push(build("smaller",1,1,128,0,false));
        cases.push(build("pan90",1,1,384,90,false));
        cases.push(build("half",1,4,384,0,true));
        app.project.bitsPerChannel=8;
        app.project.save(new File(folder.fsName+"/panorama-smoke.aep"));
        for (var j=0;j<cases.length;j++) {
            saveFrame(cases[j],new File(folder.fsName+"/"+cases[j].name+".png"));
            log("RENDER "+cases[j].name+" comp="+cases[j].width+"x"+cases[j].height);
        }
        for (var b=16;b<=32;b*=2) {
            app.project.bitsPerChannel=b;
            var c=build("depth-"+b,1,4,384,0,false);
            saveFrame(c,new File(folder.fsName+"/depth-"+b+".png"));
            log("RENDER depth-"+b);
        }
        app.project.bitsPerChannel=8;
        app.project.save(new File(folder.fsName+"/panorama-smoke.aep"));
        log("PASS: all compositions and transforms unchanged.");
    } catch(e) { log("ERROR: "+e.toString()+" line="+e.line); }
    app.endSuppressDialogs(false);
    app.quit();
}());
