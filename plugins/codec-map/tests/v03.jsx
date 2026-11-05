// SPDX-License-Identifier: MPL-2.0
// Run in an empty validation project after smoke.ps1 prepares fixtures.
(function () {
    $.evalFile(new File(File($.fileName).parent.fsName+"/controls.jsxinc"));
    var root=new Folder(File($.fileName).parent.parent.parent.parent.fsName+"/target/codec-map-v03/ui");root.create();
    var log=new File(root.fsName+"/host-report.txt");log.open("w");log.close();
    function note(s){log.open("a");log.writeln(s);log.close();}
    function save(c,name,n){c.time=n/24;var file=new File(root.fsName+"/"+name+".png");if(file.exists)file.remove();c.saveFrameToPng(c.time,file);note("requested "+name);}
    function assert(v,msg){if(!v)throw Error(msg);}
    try {
        assert(app.project.numItems===0,"Expected empty project");
        app.project.bitsPerChannel=8;
        var input=app.project.importFile(new ImportOptions(new File(root.parent.parent.fsName+"/codec-map-smoke/source.png")));
        var moving=app.project.items.addComp("0.3 moving input",129,97,1,8,24);
        var image=moving.layers.add(input);
        image.property("ADBE Transform Group").property("ADBE Position").expression="[64.5+time*35,48.5]";
        var comp=app.project.items.addComp("CodecMap 0.3",129,97,1,8,24),l=comp.layers.add(moving);
        var f=l.property("ADBE Effect Parade").addProperty("CodecMap");
        assert(codecParam(f,"Style").value===1 && codecParam(f,"Compression").value===54 && codecParam(f,"Quality Control").value===1,"New defaults");
        assert(!codecParam(f,"Compression").canVaryOverTime && codecParam(f,"Datamosh").canVaryOverTime,"Animation flags");
        save(comp,"compression",5);
        codecParam(f,"Style").setValue(2);
        assert(codecParam(f,"Codec").value===3 && codecParam(f,"Compression").value===80 && codecParam(f,"Temporal Mode").value===2,"Blocks preset");
        save(comp,"blocks",5);
        codecParam(f,"Style").setValue(3);
        assert(codecParam(f,"Codec").value===3 && codecParam(f,"Compression").value===50 && codecParam(f,"Datamosh").value===1 && codecParam(f,"GOP Length").value===12 && codecParam(f,"Allow Future Frames").value===0,"Datamosh preset");
        codecParam(f,"Datamosh").setValueAtTime(0,0);
        codecParam(f,"Datamosh").setValueAtTime(6/24,1);
        codecParam(f,"Datamosh").setValueAtTime(12/24,0);
        save(comp,"datamosh",8);save(comp,"datamosh-reset",13);
        while(codecParam(f,"Datamosh").numKeys)codecParam(f,"Datamosh").removeKey(1);
        codecParam(f,"Datamosh").setValue(0);save(comp,"datamosh-reset-reference",13);
        codecParam(f,"Style").setValue(1);codecParam(f,"Compression").setValue(0);
        for(var bits=8;bits<=32;bits*=2){app.project.bitsPerChannel=bits;save(comp,"zero"+bits,5);f.enabled=false;save(comp,"reference"+bits,5);f.enabled=true;}
        app.project.bitsPerChannel=8;
        codecParam(f,"Style").setValue(3);codecParam(f,"GOP Length").setValue(1);
        save(comp,"seed-before-edit",8);
        image.property("ADBE Transform Group").property("ADBE Position").expression="time<0.04 ? [15,48.5] : [64.5+time*35,48.5]";
        save(comp,"seed-after-edit-warm",8);app.purge(PurgeTarget.ALL_CACHES);save(comp,"seed-after-edit-cold",8);
        var large=app.project.items.addComp("4K bypass",3840,2160,1,1,24);
        var solid=large.layers.addSolid([0.1,0.3,0.6],"Input",3840,2160,1,1);
        var largeFx=solid.property("ADBE Effect Parade").addProperty("CodecMap");
        codecParam(largeFx,"Compression").setValue(0);codecParam(largeFx,"Replay Memory (MiB)").setValue(64);
        save(large,"4k-zero",0);largeFx.enabled=false;save(large,"4k-reference",0);
        comp.openInViewer();l.selected=true;
        app.project.save(new File(root.parent.fsName+"/v03.aep"));note("Completed.");
    }catch(e){note("FAIL "+e.toString()+" line "+e.line);throw e;}
}());
