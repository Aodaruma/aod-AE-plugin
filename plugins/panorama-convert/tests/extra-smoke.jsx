// Isolated regression run for reduced resolution and a catalog preview.
(function() {
    var root=File($.fileName).parent.parent.parent.parent;
    var folder=new Folder(root.fsName+"/target/panorama-smoke");
    var report=new File(folder.fsName+"/extra-report.txt");
    function log(s){report.open("a");report.writeln(s);report.close();}
    if (app.project && (app.project.numItems>0||app.project.file)){log("REFUSED");return;}
    report.open("w");report.close();
    app.beginSuppressDialogs();
    try{
        app.open(new File(folder.fsName+"/panorama-smoke.aep"));
        var source=null,half=null;
        for(var i=1;i<=app.project.numItems;i++){
            var item=app.project.item(i);
            if(item.name==="input-1.png"){source=item;}
            if(item.name==="half"){half=item;}
        }
        var rq=app.project.renderQueue.items.add(half);
        log("SETTINGS "+rq.getSettings(GetSettingsFormat.STRING_SETTABLE).toSource());
        rq.setSetting("Resolution","Half");
        rq.timeSpanStart=0;rq.timeSpanDuration=half.frameDuration;
        var om=rq.outputModule(1),template=null;
        for(var t=0;t<om.templates.length;t++){if(/PNG/i.test(om.templates[t])){template=om.templates[t];break;}}
        om.applyTemplate(template);om.file=new File(folder.fsName+"/half-render-[#####].png");
        app.project.renderQueue.render();
        var files=folder.getFiles("half-render-*.png");
        if(rq.status!==RQItemStatus.DONE||files.length!==1){throw new Error("Half render failed");}
        files[0].copy(folder.fsName+"/half.png");files[0].remove();rq.remove();
        log("PASS half render");
        var c=app.project.items.addComp("Panorama Convert Catalog",512,512,1,1,24);
        c.layers.addSolid([.025,.035,.055],"Background",512,512,1,1);
        var top=c.layers.add(source);
        top.transform.scale.setValue([160,160]);top.transform.position.setValue([256,126]);
        var bottom=c.layers.add(source);
        var fx=bottom.property("ADBE Effect Parade").addProperty("PanoramaConvert");
        fx.property("Output").setValue(4);fx.property("Output Width (0 = Input)").setValue(384);
        bottom.transform.scale.setValue([75,75]);bottom.transform.position.setValue([256,365]);
        function label(text,y){var l=c.layers.addText(text);var p=l.property("ADBE Text Properties").property("ADBE Text Document");var d=p.value;d.fontSize=16;d.fillColor=[.9,.94,1];d.justification=ParagraphJustification.CENTER_JUSTIFY;p.setValue(d);l.transform.position.setValue([256,y]);}
        label("EQUIRECTANGULAR",20);label("CUBEMAP CROSS 4:3",250);
        app.project.save(new File(folder.fsName+"/catalog-preview.aep"));
        var preview=app.project.renderQueue.items.add(c);
        preview.timeSpanStart=0;preview.timeSpanDuration=c.frameDuration;
        var pm=preview.outputModule(1);pm.applyTemplate(template);
        pm.file=new File(folder.fsName+"/catalog-render-[#####].png");
        app.project.renderQueue.render();
        var rendered=folder.getFiles("catalog-render-*.png");
        if(preview.status!==RQItemStatus.DONE||rendered.length!==1){throw new Error("Catalog render failed");}
        rendered[0].copy(folder.fsName+"/catalog.png");rendered[0].remove();preview.remove();
        log("PASS catalog preview");
    }catch(e){log("ERROR "+e.toString()+" line="+e.line);}
    app.endSuppressDialogs(false);app.quit();
}());
