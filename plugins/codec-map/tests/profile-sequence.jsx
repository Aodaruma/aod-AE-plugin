// SPDX-License-Identifier: MPL-2.0
// Launch the validation host with CODECMAP_PROFILE=1 and stderr redirected.
(function () {
    $.evalFile(new File(File($.fileName).parent.fsName + "/controls.jsxinc"));
    var root=new Folder(File($.fileName).parent.parent.parent.parent.fsName+"/target/codec-map-v03/profile");root.create();
    var c=app.project.items.addComp("CodecMap sequential profile",131,99,1,4,24);
    var source=app.project.importFile(new ImportOptions(new File(root.parent.parent.fsName+"/codec-map-smoke/source.png")));
    var l=c.layers.add(source), f=l.property("ADBE Effect Parade").addProperty("CodecMap");
    var records=[];
    for(var mode=1;mode<=2;mode++) {
        codecParam(f,"Style").setValue(mode===1?1:3);
        codecParam(f,"GOP Length").setValue(mode===1?12:1);
        var begin=(new Date()).getTime();
        for(var frame=0;frame<36;frame++) {
            c.time=frame/24;c.saveFrameToPng(c.time,new File(root.fsName+"/mode"+mode+"-"+frame+".png"));
        }
        records.push("mode="+mode+" frames=36 host_ms="+((new Date()).getTime()-begin));
    }
    var report=new File(root.fsName+"/host-report.txt");report.open("w");report.write(records.join("\n"));report.close();
    app.project.save(new File(root.parent.fsName+"/profile.aep"));
}());
