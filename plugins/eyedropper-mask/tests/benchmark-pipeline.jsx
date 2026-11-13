// Compare instrumented baseline/candidate builds and an effect-bypassed floor.
(function () {
    var root=new Folder(typeof AOD_EYEDROPPER_TEST_ROOT!=="undefined"
        ? AOD_EYEDROPPER_TEST_ROOT : File($.fileName).parent.parent.parent.parent.fsName);
    var output=new Folder(root.fsName+"/target/ae-performance-test");
    if(app.project && (app.project.file || app.project.numItems))throw new Error("Use an empty, separate AE instance");
    if(!output.exists)output.create();
    if(!app.project)app.newProject();
    function named(g,n){for(var i=1;i<=g.numProperties;i++){var p=g.property(i);if(p.name===n)return p;if(p.propertyType!==PropertyType.PROPERTY){var q=named(p,n);if(q)return q;}}return null;}
    function setup(match,label,source){
        var c=app.project.items.addComp(label,2048,1024,1,1,24),l=c.layers.add(source);
        var e=l.property("ADBE Effect Parade").addProperty(match);
        named(e,"Position").setValue([256,512]);named(e,"Opacity (%)").setValue(0);
        named(e,"Selection Source").setValue(2);named(e,"Preview Gap Radius (px)").setValue(1);
        named(e,"Stabilize Reduced Resolution").setValue(1);
        return {comp:c,layer:l,effect:e};
    }
    app.beginSuppressDialogs();
    try {
        app.project.bitsPerChannel=16;
        var source=app.project.importFile(new ImportOptions(new File(root.fsName+"/target/ae-gap-test/benchmark.tga")));
        var base=setup("EyedropperMaskPerfBase","Baseline",source);
        var next=setup("EyedropperMaskPerfTest","Optimized",source);
        var modes=[
            {name:"Quarter bypassed",item:base,factor:4,bypass:true},
            {name:"Quarter baseline",item:base,factor:4},
            {name:"Quarter optimized",item:next,factor:4},
            {name:"Full baseline",item:base,factor:1},
            {name:"Full optimized",item:next,factor:1}
        ];
        var rows=["Mode,Trial,Milliseconds"],times=[[],[],[],[],[]];
        for(var round=0;round<8;round++)for(var step=0;step<modes.length;step++){
            var index=(round+step)%modes.length,mode=modes[index],item=mode.item;
            item.comp.resolutionFactor=[mode.factor,mode.factor];item.effect.enabled=!mode.bypass;
            item.layer.property("ADBE Transform Group").property("ADBE Opacity").setValue(mode.bypass?99+round*0.01:100);
            named(item.effect,"Color Threshold (%)").setValue(20+(round*modes.length+step)*0.05);
            var q=app.project.renderQueue.items.add(item.comp);q.timeSpanDuration=1/24;
            q.setSetting("Resolution",mode.factor===1?"Full":"Quarter");
            q.outputModule(1).applyTemplate("PNG seq (untone-map)");
            q.outputModule(1).file=new File(output.fsName+"/pipeline-"+round+"-"+index+"_[#####].png");
            var start=(new Date()).getTime();app.project.renderQueue.render();
            var elapsed=(new Date()).getTime()-start,complete=q.status===RQItemStatus.DONE;
            q.remove();if(!complete)throw new Error("Render did not complete: "+mode.name);
            if(round){times[index].push(elapsed);rows.push(mode.name+","+round+","+elapsed);}
        }
        var report=new File(output.fsName+"/pipeline.csv");report.open("w");report.write(rows.join("\n"));report.close();
        var summary=[];for(var i=0;i<modes.length;i++){times[i].sort(function(a,b){return a-b;});summary.push(modes[i].name+": "+times[i][3]+" ms");}
        report=new File(output.fsName+"/pipeline.txt");report.open("w");report.write("AE "+app.version+" / 16 bpc / 7 trials, warm-up excluded / render + PNG\n"+summary.join("\n"));report.close();
        app.project.save(new File(output.fsName+"/pipeline.aep"));return summary;
    } finally {app.endSuppressDialogs(false);}
})();
