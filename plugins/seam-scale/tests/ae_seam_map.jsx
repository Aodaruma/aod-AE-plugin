// Run after the smoke test, only in the authorized test project.
(function () {
    var root = new Folder($.global.AOD_SEAM_TEST_ROOT || "");
    if (!new File(root.fsName + "/plugins/seam-scale/Cargo.toml").exists ||
        !app.project.file || !/^seam-(scale-smoke|map-preview)\.aep$/.test(app.project.file.name) ||
        app.project.file.fsName.toLowerCase().indexOf((root.fsName + "/target/").replace(/\//g,"\\").toLowerCase()) !== 0) {
        throw new Error("Open the saved SeamScale smoke test project in this worktree.");
    }
    var out = new Folder(root.fsName + "/target/seam-scale-ae-0.2.0/seam-map");
    if (!out.exists && !out.create()) { throw new Error("Cannot create test output directory."); }
    var log = new File(out.fsName + "/result.txt");
    if (!log.open("w")) { throw new Error("Cannot open test log."); }
    function record(s) { log.writeln(s); log.close(); log.open("a"); }
    var w=64, h=48;
    function comp(name) { return app.project.items.addComp(name,w,h,1,1,24); }
    function save(c,name) {
        var item=app.project.renderQueue.items.add(c);
        item.applyTemplate("Best Settings");
        item.timeSpanStart=0; item.timeSpanDuration=1/24;
        var module=item.outputModule(1);
        module.applyTemplate("PNG seq (untone-map)");
        module=item.outputModule(1);
        module.file=new File(out.fsName+"/"+name+"_[#####].png");
    }
    function seam(c,source,settings,mode) {
        var layer=c.layers.add(source);
        var e=layer.property("ADBE Effect Parade").addProperty("SeamScale");
        e.property("Separate X/Y Scale").setValue(1);
        e.property("Scale X (%)").setValue(settings[1]);
        e.property("Scale Y (%)").setValue(settings[2]);
        e.property("Restore Original Size").setValue(settings[3]);
        e.property("Output").setValue(mode);
        return layer;
    }
    app.beginSuppressDialogs();
    try {
        record("AE "+app.version+"; Seam Map; 64x48; RGBA; native Displacement Map + Set Matte");
        for(var q=1;q<=app.project.renderQueue.numItems;q++) {
            if(app.project.renderQueue.item(q).status===RQItemStatus.QUEUED) {
                app.project.renderQueue.item(q).render=false;
            }
        }
        var source=comp("Seam Map Source");
        var bg=source.layers.addSolid([0.1,0.2,0.4],"Background",w,h,1);
        var ramp=bg.property("ADBE Effect Parade").addProperty("ADBE Ramp");
        ramp.property(1).setValue([0,0]); ramp.property(2).setValue([0.1,0.2,0.4,1]);
        ramp.property(3).setValue([w,h]); ramp.property(4).setValue([0.7,0.4,0.2,1]);
        for(var i=0;i<5;i++) {
            var strip=source.layers.addSolid([0.1+i*0.17,0.8-i*0.12,0.2+i*0.1],"Strip "+i,5,14+i*5,1);
            strip.position.setValue([8+i*11,16+i*4]);
        }
        var cases=[
            ["identity",100,100,0], ["shrink",50,50,0], ["x-only",75,100,0],
            ["flip-x",-50,75,0], ["flip-xy",-50,-75,0], ["restore",50,75,1],
            ["expand",150,125,0], ["expand-restored",150,125,1], ["zero",0,100,0]
        ];
        var preview=null;
        for(var di=0;di<3;di++) {
            var depth=[8,16,32][di]; app.project.bitsPerChannel=depth;
            save(source,"source-"+depth);
            for(var ci=0;ci<cases.length;ci++) {
                var settings=cases[ci], name=settings[0]+"-"+depth;
                var direct=comp("Direct "+name); seam(direct,source,settings,1);
                var map=comp("Seam Map "+name); seam(map,source,settings,2);
                var warped=comp("Displacement "+name);
                var mapLayer=warped.layers.add(map); mapLayer.enabled=false;
                var target=warped.layers.add(source);
                var displace=target.property("ADBE Effect Parade").addProperty("ADBE Displacement Map");
                displace.property(1).setValue(mapLayer.index);
                displace.property(2).setValue(1); displace.property(3).setValue(w);
                displace.property(4).setValue(2); displace.property(5).setValue(h);
                displace.property(7).setValue(0); displace.property(8).setValue(0);
                var matte=target.property("ADBE Effect Parade").addProperty("ADBE Set Matte3");
                matte.property(1).setValue(mapLayer.index); matte.property(2).setValue(3);
                matte.property(5).setValue(1); matte.property(6).setValue(0);
                save(direct,"direct-"+name); save(map,"map-"+name); save(warped,"warped-"+name);
                record("rendered "+name);
                if(depth===32 && settings[0]==="shrink") { preview=map; }
            }
            app.project.renderQueue.render();
            for(var ri=app.project.renderQueue.numItems-27;ri<=app.project.renderQueue.numItems;ri++) {
                if(app.project.renderQueue.item(ri).status!==RQItemStatus.DONE) {
                    throw new Error("Incomplete render at "+depth+" bpc");
                }
            }
            record("PASS 28 untone-mapped 16-bit PNGs at "+depth+" bpc");
        }
        app.project.bitsPerChannel=32;
        app.project.save(new File(out.fsName+"/seam-map-preview.aep"));
        preview.openInViewer(); preview.layer(1).selected=true;
        record("PASS all Seam Map render calls completed");
    } catch(error) { record("FAIL "+error.toString()+" line="+error.line); }
    finally { log.close(); app.endSuppressDialogs(false); }
}());
