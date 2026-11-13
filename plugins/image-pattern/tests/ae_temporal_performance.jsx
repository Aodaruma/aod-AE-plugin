(function () {
    var root = new Folder(File($.fileName).parent.parent.parent.parent.fsName);
    var output = new Folder(root.fsName + "/target/ae-image-pattern-performance");
    if (!output.exists) { output.create(); }
    var report = new File(output.fsName + "/report.txt"); report.open("w");
    function log(s) { report.writeln(s); report.close(); report.open("a"); }
    var created = [], depth = app.project.bitsPerChannel;
    try {
        app.project.bitsPerChannel = 8;
        var source = app.project.items.addComp("__IP_Perf_Source", 640, 360, 1, 5, 24); created.push(source);
        for (var i = 0; i < 8; i++) {
            var cell = source.layers.addSolid([1,1,1], "Cell", 80, 360, 1, 5); created.push(cell.source);
            cell.position.setValue([40+i*80,180]); cell.opacity.setValue(60);
            cell.property("ADBE Effect Parade").addProperty("ADBE Fill").property("ADBE Fill-0002").expression = "["+(i+1)/8+",time/5,0.5,1]";
        }
        var comp = app.project.items.addComp("__IP_Perf", 640, 360, 1, 5, 24); created.push(comp);
        var fx = comp.layers.add(source).property("ADBE Effect Parade").addProperty("ImagePattern");
        function set(n,v) { fx.property(n).setValue(v); }
        set("Pattern Mode",3); set("Angle",8); set("Width (px)",32);
        set("Copies Each Side",32); set("Delta Angle",4); set("Delta Width (px)",1);
        set("Delta X (px)",3); set("Time Delta (frames)",1);
        var cases = [
            ["closed",1,1,1], ["open",2,1,1], ["open-held",4,1,120],
            ["open-unsplit",2,0,1]
        ];
        for (var c = 0; c < cases.length; c++) {
            var test = cases[c]; set("Clipping",test[1]); set("Split Overlaps",test[2]); set("Time Delta (frames)",test[3]);
            for (var r = 0; r < 3; r++) {
                app.purge(PurgeTarget.ALL_CACHES);
                var file = new File(output.fsName+"/"+test[0]+"-"+r+".png"); if(file.exists) { file.remove(); }
                var start = new Date().getTime(); comp.saveFrameToPng(2+r/24,file);
                while(!file.exists || file.length === 0) { if(new Date().getTime()-start>180000) { throw new Error("Timeout"); } $.sleep(25); file=new File(file.fsName); }
                log(test[0]+","+r+","+(new Date().getTime()-start));
            }
        }
        log("RESULT=PASS");
    } catch(e) { log("RESULT=FAIL "+e+" line "+e.line); }
    finally { app.project.bitsPerChannel=depth; for(var i=created.length-1;i>=0;i--) { try { created[i].remove(); } catch(e) {} } report.close(); }
}());
