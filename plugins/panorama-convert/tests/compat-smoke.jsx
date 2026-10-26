// Run in an isolated, empty AE process with the 0.2 fixture in target/panorama-smoke.
(function () {
    var root=File($.fileName).parent.parent.parent.parent;
    var folder=new Folder(root.fsName+"/target/panorama-smoke");
    var report=new File(folder.fsName+"/compat-report.txt");
    function log(s){report.open("a");report.writeln(s);report.close();}
    function check(ok,msg){if(!ok){throw new Error(msg);}}
    if(app.project&&(app.project.numItems||app.project.file)){log("REFUSED: nonempty project");return;}
    report.open("w");report.close();
    app.beginSuppressDialogs();
    try {
        app.open(new File(folder.fsName+"/compat-0.2.aep"));
        var count=0,renderComp=null;
        var names=["Input","Output","Output Width (0 = Input)","Pan (Y)","Tilt (X)","Roll (Z)","Interpolation","Antialiasing"];
        var ids=["703497118","1518570117","-2060184151","1875716666","1597018293","1989305725","1493514260","78734759"];
        for(var i=1;i<=app.project.numItems;i++) {
            var c=app.project.item(i);
            if(!(c instanceof CompItem)){continue;}
            var e=c.layer(1).property("ADBE Effect Parade").property(1);
            check(e.matchName==="PanoramaConvert","Wrong effect");
            check(!e.property("Operation")&&!e.property("Axis Point")&&!e.property("Axis Twist"),"Removed controls remain");
            for(var j=0;j<names.length;j++) {
                check(e.property(j+1).name===names[j],"UI order differs: "+names[j]);
                check(e.property(names[j]).matchName==="PanoramaConvert-"+ids[j],"Persistent ID changed: "+names[j]);
            }
            var match=/^map-(\d)-(\d)$/.exec(c.name);
            if(match) {
                check(e.property("Input").value===Number(match[1]),"Input changed: "+c.name);
                check(e.property("Output").value===Number(match[2]),"Output changed: "+c.name);
                check(e.property("Output Width (0 = Input)").value===384,"Width changed: "+c.name);
                count++;
            }
            if(c.name==="pan90"){check(e.property("Pan (Y)").value===90,"Pan changed");}
            if(c.name==="map-1-4"){renderComp=c;}
            check(c.width===640&&c.height===480,"Composition resized");
        }
        check(count===25&&renderComp,"Missing conversion cases");
        log("PASS: old projection settings and 8 persistent parameter IDs preserved; Operation and axis controls absent.");
        var item=app.project.renderQueue.items.add(renderComp);
        item.timeSpanStart=0;item.timeSpanDuration=renderComp.frameDuration;
        item.setSetting("Resolution","Full");
        var om=item.outputModule(1),template=null;
        for(var t=0;t<om.templates.length;t++){if(/PNG/i.test(om.templates[t])){template=om.templates[t];break;}}
        check(template,"No PNG template");om.applyTemplate(template);
        var prefix="compat-"+new Date().getTime();
        om.file=new File(folder.fsName+"/"+prefix+"-[#####].png");
        app.project.renderQueue.render();
        var files=folder.getFiles(prefix+"-*.png"),dest=new File(folder.fsName+"/compat-output.png");
        check(item.status===RQItemStatus.DONE&&files.length===1,"Render failed");
        if(dest.exists){dest.remove();}check(files[0].copy(dest.fsName),"Copy failed");files[0].remove();item.remove();
        log("PASS: old projection rendered successfully.");
    } catch(e) {log("ERROR "+e.toString()+" line="+e.line);}
    app.endSuppressDialogs(false);app.quit();
}());
