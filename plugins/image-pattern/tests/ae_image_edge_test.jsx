(function () {
    var root = new Folder(File($.fileName).parent.parent.parent.parent.fsName);
    var output = new Folder(root.fsName + "/target/ae-image-pattern-edge");
    if (!output.exists) { output.create(); }
    var report = new File(output.fsName + "/report.txt"); report.encoding = "UTF-8"; report.open("w");
    function log(s) { report.writeln(s); report.close(); report.open("a"); }
    function set(n,v) { fx.property(n).setValue(v); }
    function save(comp,name,time) {
        var f = new File(output.fsName + "/" + name + ".png"); if(f.exists) { f.remove(); }
        comp.saveFrameToPng(time,f);
        var deadline = new Date().getTime()+30000;
        do { f=new File(f.fsName); if(f.exists && f.length>0) { log("PASS "+name); return; } $.sleep(25); } while(new Date().getTime()<deadline);
        throw new Error("Render timeout "+name);
    }
    var created=[], depth=app.project.bitsPerChannel;
    try {
        var source=app.project.items.addComp("__IP046_Source",128,96,1,5,24); created.push(source);
        for(var y=0;y<3;y++) { for(var x=0;x<4;x++) {
            var cell=source.layers.addSolid([1,1,1],"Cell",32,32,1,5); created.push(cell.source);
            cell.position.setValue([16+x*32,16+y*32]);
            if(x===0 && y===1) { cell.opacity.setValue(0); }
            cell.property("ADBE Effect Parade").addProperty("ADBE Fill").property("ADBE Fill-0002").expression="["+(x+1)/4+","+(y+1)/3+",time/5,1]";
        } }
        var comp=app.project.items.addComp("__IP046_Edge",128,96,1,5,24); created.push(comp);
        var fx=comp.layers.add(source).property("ADBE Effect Parade").addProperty("ImagePattern");
        if(fx.property("Edge").value!==1 || fx.property("Edge").maxValue!==5 || fx.property("Time Edge").maxValue!==4) { throw new Error("Invalid edge choices/defaults"); }
        set("Pattern Mode",3); set("Angle",0); set("Width (px)",32); set("Copies Each Side",1);
        var depths=[8,16,32];
        for(var d=0;d<depths.length;d++) {
            app.project.bitsPerChannel=depths[d]; var suffix="-"+depths[d];
            for(var t=-1;t<=1;t++) { save(source,"source-"+t+suffix,2+t*0.5); }
            for(var sampling=1;sampling<=2;sampling++) {
                set("Sampling",sampling); set("Delta X (px)",sampling===1?80:80.25);
                for(var time=0;time<=1;time++) {
                    set("Time Delta (frames)",time*12);
                    for(var direction=1;direction<=2;direction++) {
                        set("Edge Direction",direction);
                        for(var edge=1;edge<=5;edge++) {
                            set("Edge",edge); save(comp,"edge-"+edge+"-"+direction+"-"+sampling+"-"+time+suffix,2);
                        }
                    }
                }
            }
            set("Sampling",1); set("Delta X (px)",80); set("Edge",5); set("Clipping",4);
            for(var time=0;time<=1;time++) {
                set("Time Delta (frames)",time*12);
                for(var order=1;order<=2;order++) { set("Overlap Order",order); save(comp,"overlap-"+order+"-"+time+suffix,2); }
            }
            set("Clipping",1); set("Overlap Order",1);
        }
        app.project.save(new File(output.fsName+"/verification.aep"));
        log("RESULT=PASS");
    } catch(e) { log("RESULT=FAIL "+e+" line "+e.line); }
    finally { app.project.bitsPerChannel=depth; for(var i=created.length-1;i>=0;i--) { try { created[i].remove(); } catch(e) {} } report.close(); }
}());
