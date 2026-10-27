// End-to-end AE render + PNG export timing, not isolated effect profiling.
(function () {
    var root=new Folder(typeof AOD_EYEDROPPER_TEST_ROOT!=="undefined"
        ? AOD_EYEDROPPER_TEST_ROOT : File($.fileName).parent.parent.parent.parent.fsName);
    var output=new Folder(root.fsName+"/target/ae-gap-test");
    if(!app.project.file || app.project.file.fsName!==new File(output.fsName+"/gap-smoke.aep").fsName) {throw new Error("Use the dedicated gap-smoke project");}
    if(app.project.renderQueue.numItems) {throw new Error("Use an empty render queue");}
    function named(g,n){for(var i=1;i<=g.numProperties;i++){var p=g.property(i);if(p.name===n)return p;if(p.propertyType!==PropertyType.PROPERTY){var q=named(p,n);if(q)return q;}}return null;}
    var match="EyedropperMask";
    for(var i=0;i<app.effects.length;i++){if(app.effects[i].matchName==="EyedropperMaskGapTest")match="EyedropperMaskGapTest";}
    app.beginSuppressDialogs();
    try {
        app.project.bitsPerChannel=16;
        var file=new File(output.fsName+"/benchmark.tga");file.encoding="BINARY";file.open("w");
        file.write(String.fromCharCode(0,0,2,0,0,0,0,0,0,0,0,0,0,8,0,4,32,40));
        var normal="", shifted="";
        for(var x=0;x<2048;x++){
            normal+=String.fromCharCode(0,0,x>=1022&&x<=1025?0:255,255);
            shifted+=String.fromCharCode(0,0,x>=1024&&x<=1025?0:255,255);
        }
        for(var y=0;y<1024;y++)file.write(y>=508&&y<512?shifted:normal);
        file.close();
        var comp=app.project.items.addComp("Gap Performance",2048,1024,1,1,24);
        var footage=app.project.importFile(new ImportOptions(file));
        var layer=comp.layers.add(footage);
        var effect=layer.property("ADBE Effect Parade").addProperty(match);
        named(effect,"Position").setValue([256,512]);named(effect,"Opacity (%)").setValue(0);
        named(effect,"Selection Source").setValue(2);named(effect,"Preview Gap Radius (px)").setValue(1);
        var rows=["Mode,Trial,Milliseconds"],times=[[],[],[]],labels=["Full","Quarter OFF","Quarter ON"];
        // Rotate order and vary threshold to invalidate this effect's output.
        // The source cache can remain warm for all modes. Discard warm-up round.
        for(var round=0;round<8;round++)for(var step=0;step<3;step++){
            var mode=(round+step)%3;
            comp.resolutionFactor=mode===0?[1,1]:[4,4];
            named(effect,"Stabilize Reduced Resolution").setValue(mode===2?1:0);
            named(effect,"Color Threshold (%)").setValue(20+(round*3+step)*0.1);
            var item=app.project.renderQueue.items.add(comp);
            item.timeSpanDuration=1/24;
            item.setSetting("Resolution",mode===0?"Full":"Quarter");
            item.outputModule(1).applyTemplate("PNG seq (untone-map)");
            item.outputModule(1).file=new File(output.fsName+"/rq-benchmark-"+round+"-"+mode+"_[#####].png");
            var start=(new Date()).getTime();
            app.project.renderQueue.render();
            var elapsed=(new Date()).getTime()-start;
            var complete=item.status===RQItemStatus.DONE;
            item.remove();
            if(!complete) {throw new Error("Benchmark render did not complete");}
            if(round>0){times[mode].push(elapsed);rows.push(labels[mode]+","+round+","+elapsed);}
        }
        var report=new File(output.fsName+"/host-benchmark.csv");report.open("w");report.write(rows.join("\n"));report.close();
        var summary=[];
        for(var mode=0;mode<3;mode++){times[mode].sort(function(a,b){return a-b;});summary.push(labels[mode]+": median "+times[mode][3]+" ms");}
        var note=new File(output.fsName+"/host-benchmark.txt");note.open("w");note.write("AE "+app.version+" / 2048x1024 / 16 bpc / 7 trials per mode / render + PNG export\n"+summary.join("\n"));note.close();
        app.project.save();return summary;
    } finally {app.endSuppressDialogs(false);}
})();
