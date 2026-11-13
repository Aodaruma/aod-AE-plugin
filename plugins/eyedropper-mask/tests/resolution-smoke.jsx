// Run in an empty, separate AE instance with the candidate plug-in installed.
(function () {
    var root = new Folder(typeof AOD_EYEDROPPER_TEST_ROOT !== "undefined"
        ? AOD_EYEDROPPER_TEST_ROOT : File($.fileName).parent.parent.parent.parent.fsName);
    var output = new Folder(typeof AOD_EYEDROPPER_TEST_OUTPUT !== "undefined"
        ? AOD_EYEDROPPER_TEST_OUTPUT : root.fsName + "/target/ae-gap-test");
    if (!output.exists && !output.create()) { throw new Error("Cannot create output folder"); }
    if (app.project && (app.project.file || app.project.numItems)) { throw new Error("Use an empty, separate AE instance"); }
    if (!app.project) { app.newProject(); }
    var records = ["Case,Depth,Source,Stable,Radius,Factor,ExpectedRightAlpha"];
    var matchName = "EyedropperMask";
    for (var i=0;i<app.effects.length;i++) {
        if(app.effects[i].matchName === "EyedropperMaskGapTest") { matchName="EyedropperMaskGapTest"; }
    }
    if(typeof AOD_EYEDROPPER_TEST_MATCH_NAME !== "undefined") { matchName=AOD_EYEDROPPER_TEST_MATCH_NAME; }
    function named(group, name) {
        for(var i=1;i<=group.numProperties;i++) {
            var p=group.property(i);
            if(p.name===name) { return p; }
            if(p.propertyType!==PropertyType.PROPERTY) { var found=named(p,name); if(found) { return found; } }
        }
        return null;
    }
    function raster(name, kind, alpha) {
        var file = new File(output.fsName + "/" + name + ".tga");
        file.encoding="BINARY";
        if(!file.open("w")) { throw new Error("Cannot write fixture"); }
        file.write(String.fromCharCode(0,0,2,0,0,0,0,0,0,0,0,0,0,1,128,0,32,40));
        for(var y=0;y<128;y++) {
            var row="";
            for(var x=0;x<256;x++) {
                // A closed wall shifts locally. Quarter sampling drops just
                // that short segment, opening a path into the other region.
                var shifted=(kind==="narrow" && y>=60 && y<64) || (kind==="medium" && y>=48 && y<64);
                var wall=x>=(shifted?128:126) && x<=129;
                if(kind==="wide" && y>=48 && y<80) { wall=false; }
                if(kind==="vanished") { wall=x===128; }
                row+=String.fromCharCode(0,0,wall?0:255,alpha);
            }
            file.write(row);
        }
        file.close();
        var footage=app.project.importFile(new ImportOptions(file));
        footage.mainSource.alphaMode=AlphaMode.STRAIGHT;
        return footage;
    }
    function setup(name, source, selection, fill) {
        var comp=app.project.items.addComp(name,256,128,1,1,24);
        var layer=comp.layers.add(source);
        if(fill) { layer.property("ADBE Effect Parade").addProperty("ADBE Fill").property("ADBE Fill-0002").setValue([0,1,0,1]); }
        var effect=layer.property("ADBE Effect Parade").addProperty(matchName);
        if(!named(effect,"Preview Gap Radius (px)")) { throw new Error("Approximate candidate not installed"); }
        named(effect,"Position").setValue([32,64]);
        named(effect,"Opacity (%)").setValue(0);
        named(effect,"Color Threshold (%)").setValue(20);
        named(effect,"Selection Source").setValue(selection);
        return {comp:comp,effect:effect,selection:selection};
    }
    function render(item,name,depth,stable,radius,factor,expected) {
        app.project.bitsPerChannel=depth;
        named(item.effect,"Stabilize Reduced Resolution").setValue(stable);
        named(item.effect,"Preview Gap Radius (px)").setValue(radius);
        item.comp.resolutionFactor=[factor,factor];
        item.comp.saveFrameToPng(0,new File(output.fsName+"/"+name+".png"));
        records.push([name,depth,item.selection,stable,radius,factor,expected].join(","));
        var report=new File(output.fsName+"/host-cases.csv");
        if(!report.open("w")) { throw new Error("Cannot write manifest"); }
        report.write(records.join("\n"));report.close();
    }
    app.beginSuppressDialogs();
    try {
        var narrow=raster("narrow","narrow",255);
        for(var b=0;b<3;b++) {
            var depth=[8,16,32][b];
            for(var source=1;source<=2;source++) {
                var item=setup("Narrow "+depth+" "+source,narrow,source,false);
                for(var stable=0;stable<=1;stable++) {
                    for(var factor=1;factor<=4;factor++) {
                        // Record intermediate resolutions; assert both the
                        // Full reference and the targeted Quarter regression.
                        var expected=(factor===1 || stable===1)?255:(factor===4?0:-1);
                        render(item,"narrow-bpc"+depth+"-source"+source+"-stable"+stable+"-res"+factor,depth,stable,1,factor,expected);
                    }
                }
            }
        }
        var semi=raster("semi","narrow",128);
        for(var source=1;source<=2;source++) {
            for(var color=1;color<=2;color++) {
                var item=setup("Semi "+source+" "+color,semi,source,false);
                named(item.effect,"Color Interpretation").setValue(color);
                render(item,"semi-source"+source+"-color"+color+"-full",8,1,1,1,128);
                render(item,"semi-source"+source+"-color"+color+"-quarter",8,1,1,4,128);
            }
        }
        for(var k=0;k<2;k++) {
            var kind=["wide","vanished"][k];
            var item=setup(kind,raster(kind,kind,255),2,false);
            for(var stable=0;stable<=1;stable++) {
                render(item,kind+"-stable"+stable+"-full",8,stable,1,1,kind==="wide"?0:255);
                render(item,kind+"-stable"+stable+"-quarter",8,stable,1,4,0);
            }
        }
        var item=setup("Radius",raster("medium","medium",255),2,false);
        render(item,"radius1-full",8,1,1,1,255);
        render(item,"radius1-quarter",8,1,1,4,0);
        render(item,"radius2-quarter",8,1,2,4,255);
        for(var source=1;source<=2;source++) {
            var item=setup("Fill "+source,narrow,source,true);
            render(item,"fill-source"+source+"-full",8,1,1,1,source===1?255:0);
            render(item,"fill-source"+source+"-quarter",8,1,1,4,source===1?255:0);
        }
        app.project.bitsPerChannel=8;
        app.project.save(new File(output.fsName+"/gap-smoke.aep"));
        return "AE "+app.version+": exported "+(records.length-1)+" cases with "+matchName;
    } finally { app.endSuppressDialogs(false); }
})();
