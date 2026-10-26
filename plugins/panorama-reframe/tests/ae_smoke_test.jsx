// Run in a separate empty AE instance: AfterFX.com -m -noui -ro <this file>.
(function () {
    app.exitAfterLaunchAndEval = true;
    var root = new Folder(File($.fileName).parent.parent.parent.parent.fsName);
    var out = new Folder(root.fsName + '/.task/ae');
    if (!out.exists) { throw new Error('Generate fixtures first.'); }
    var log = new File(out.fsName+'/result.txt'); log.open('w');
    function record(s) { log.writeln(s); log.close(); log.open('a'); }
    function save(c,name,time) {
        var file=new File(out.fsName+'/'+name+'.png'); if(file.exists) file.remove();
        c.saveFrameToPng(time || 0,file);
        var deadline=new Date().getTime()+15000;
        while((!file.exists || file.length===0) && new Date().getTime()<deadline) {$.sleep(50);file=new File(file.fsName);}
        if(!file.exists || file.length===0) throw new Error('PNG missing: '+name);
        record('rendered '+name);
    }
    app.beginSuppressDialogs();
    try {
        if(app.project.numItems!==0) { throw new Error('Use a separate empty AE process.'); }
        if(parseInt(app.version,10)!==25) { throw new Error('Run this verification in After Effects 2025.'); }
        record('AE 2025 version '+app.version);
        app.project.workingSpace='';
        var items={};
        function footage(name) { if(!items[name]) {items[name]=app.project.importFile(new ImportOptions(new File(out.fsName+'/'+name+'.png')));} return items[name]; }
        function fixture(name,input,proj,layout,par,inputSize) {
            var item=footage(input);
            var w=inputSize?inputSize[0]:(proj>=2?item.width:320);
            var h=inputSize?inputSize[1]:(proj>=2?item.height:180);
            // Match the input canvas to the test's output aspect, retaining square cube faces.
            var prepared=app.project.items.addComp(name+' Input',w,h,par || 1,2,24);
            var src=prepared.layers.add(item);
            src.property('ADBE Transform Group').property('ADBE Scale').setValue([100*w*(par || 1)/(item.width*item.pixelAspect),100*h/item.height]);
            var c=app.project.items.addComp(name,320,180,par || 1,2,24);
            var layer=c.layers.add(prepared);layer.name=name;
            layer.property('ADBE Transform Group').property('ADBE Scale').setValue([32000/w,18000/h]);
            var e=layer.property('ADBE Effect Parade').addProperty('PanoramaReframe');
            if(e.property('Panorama Layer'))throw new Error('Panorama Layer must be removed');
            e.property('Input Projection').setValue(proj || 1);
            e.property('Cube Layout').setValue(layout || 1);
            return {comp:c,layer:layer,effect:e};
        }
        var a=fixture('Main','latlong'); var e=a.effect;
        for(var depth=8;depth<=32;depth*=2) {app.project.bitsPerChannel=depth;save(a.comp,'perspective-'+depth);}
        app.project.bitsPerChannel=32;
        var probe=a.comp.layers.addNull();probe.name='Probe';
        var color=probe.property('ADBE Effect Parade').addProperty('ADBE Color Control').property(1);
        color.expression='thisComp.layer("Main").sampleImage([160,90],[.5,.5],true,0)';
        record('float sample '+color.value.toString());probe.remove();
        var raw=app.project.items.addComp('Raw Source',1024,512,1,1,24);raw.layers.add(footage('latlong'));save(raw,'raw32');
        var direct=app.project.items.addComp('Direct Footage',1024,512,1,2,24);
        direct.layers.add(footage('latlong')).property('ADBE Effect Parade').addProperty('PanoramaReframe');
        save(direct,'direct-footage');
        var cropped=app.project.items.addComp('Direct Footage Cropped',512,512,1,2,24);
        cropped.layers.add(footage('latlong')).property('ADBE Effect Parade').addProperty('PanoramaReframe');
        save(cropped,'direct-cropped');
        for(var pb=8;pb<=32;pb*=2){
            app.project.bitsPerChannel=pb;
            var before=fixture('Before Invert '+pb,'latlong');
            before.layer.property('ADBE Effect Parade').addProperty('ADBE Invert').moveTo(1);
            save(before.comp,'pipeline-before-invert-'+pb);
            var after=fixture('After Invert '+pb,'latlong');
            after.layer.property('ADBE Effect Parade').addProperty('ADBE Invert');
            save(after.comp,'pipeline-after-invert-'+pb);
            var masked=fixture('Input Mask '+pb,'latlong');
            var mask=masked.layer.property('ADBE Mask Parade').addProperty('ADBE Mask Atom');
            var shape=new Shape();shape.vertices=[[160,0],[320,0],[320,180],[160,180]];shape.closed=true;
            mask.property('ADBE Mask Shape').setValue(shape);
            save(masked.comp,'pipeline-mask-'+pb);
        }
        app.project.bitsPerChannel=32;
        record('PASS native input, preceding effects and masks');
        e.property('Rotation Y').setValue(90);save(a.comp,'yaw90');
        e.property('Rotation Y').setValue(0);e.property('Rotation Mode').setValue(2);
        e.property('Quaternion W').setValue(Math.sqrt(.5));e.property('Quaternion Y').setValue(Math.sqrt(.5));save(a.comp,'quaternion90');
        e.property('Quaternion W').setValue(1);e.property('Quaternion Y').setValue(0);e.property('Rotation Mode').setValue(1);
        e.property('Output Projection').setValue(2);e.property('Field of View').setValue(180);save(a.comp,'fisheye');
        e.property('Output Projection').setValue(3);e.property('Orthographic Width').setValue(40000);save(a.comp,'orthographic');
        e.property('Output Projection').setValue(4);e.property('Orthographic Width').setValue(10000);save(a.comp,'isometric');
        e.property('Output Projection').setValue(1);e.property('Field of View').setValue(90);
        e.property('Position X').setValue(5000);save(a.comp,'position');e.property('Position X').setValue(0);
        app.project.bitsPerChannel=8;
        e.property('Position X').setValue(5000);save(a.comp,'environment-half-depth');e.property('Position X').setValue(0);
        var axes=['X','Y','Z'];
        for(var axis=0;axis<3;axis++) { for(var sign=-1;sign<=1;sign+=2) {
            e.property('Position '+axes[axis]).setValue(sign*30000);
            save(a.comp,'environment-'+axes[axis]+'-'+(sign>0?'plus':'minus'));
            e.property('Position '+axes[axis]).setValue(0);
        }}
        if(e.property('Position Mapping') || e.property('Sphere Center X')) throw new Error('Legacy controls remain');
        var dp=fixture('Depth map tests','latlong');var de=dp.effect;
        if(de.property('Depth Map Layer').value!==0) throw new Error('Depth must default to None');
        de.property('Position X').setValue(500);
        var depthLayers={};
        function setDepth(name) {
            if(!depthLayers[name]) { var layer=dp.comp.layers.add(footage('depth-'+name));layer.enabled=false;depthLayers[name]=layer; }
            de.property('Depth Map Layer').setValue(depthLayers[name].index);
        }
        for(var bits=8;bits<=32;bits*=2) {
            app.project.bitsPerChannel=bits;
            de.property('Depth Map Layer').setValue(0);de.property('Parallax Distance').setValue(1000);save(dp.comp,'depth-near-reference-'+bits);
            de.property('Parallax Distance').setValue(10000);save(dp.comp,'depth-far-reference-'+bits);
            setDepth('near');save(dp.comp,'depth-near-'+bits);
            setDepth('far');save(dp.comp,'depth-far-'+bits);
            setDepth('split');save(dp.comp,'depth-split-'+bits);
            setDepth('transparent');save(dp.comp,'depth-transparent-'+bits);
            de.property('Depth Map Layer').setValue(0);save(dp.comp,'depth-disabled-'+bits);
        }
        app.project.bitsPerChannel=8;
        setDepth('near');de.property('Invert Depth').setValue(1);save(dp.comp,'depth-inverted');de.property('Invert Depth').setValue(0);
        setDepth('split');de.property('Position X').setValue(0);save(dp.comp,'depth-zero');
        de.property('Position X').setValue(500);dp.comp.resolutionFactor=[2,2];save(dp.comp,'depth-half');dp.comp.resolutionFactor=[1,1];
        de.property('Position X').setValue(30000);save(dp.comp,'depth-extreme');de.property('Position X').setValue(500);
        var animated=app.project.items.addComp('Animated Depth',256,128,1,2,24);
        animated.layers.addSolid([1,1,1],'Far',256,128,1,2);
        var cover=animated.layers.addSolid([0,0,0],'Near',256,128,1,2);
        var opacity=cover.property('ADBE Transform Group').property('ADBE Opacity');opacity.setValueAtTime(0,100);opacity.setValueAtTime(1,0);
        var animatedLayer=dp.comp.layers.add(animated);animatedLayer.enabled=false;de.property('Depth Map Layer').setValue(animatedLayer.index);
        save(dp.comp,'depth-animated-near',0);save(dp.comp,'depth-animated-far',1);
        de.property('Depth Map Layer').setValue(0);save(dp.comp,'depth-removed-after-animation');
        app.project.save(new File(out.fsName+'/panorama-reframe-tests.aep'));
        record('PASS depth controls and automatic enable/disable');
        app.project.bitsPerChannel=32;
        for(var filter=1;filter<=7;filter++) {e.property('Interpolation').setValue(filter);save(a.comp,'filter-'+filter);}
        e.property('Interpolation').setValue(2);
        a.comp.resolutionFactor=[2,2];save(a.comp,'half');a.comp.resolutionFactor=[1,1];
        var cam=a.comp.layers.addCamera('Camera',[160,90]);
        cam.property('ADBE Transform Group').property('ADBE Position').setValue([160,90,-160]);
        cam.property('ADBE Transform Group').property('ADBE Anchor Point').setValue([160,90,0]);
        cam.property('ADBE Camera Options Group').property('ADBE Camera Zoom').setValue(160);
        e.property('Sync AE Camera').setValue(1);save(a.comp,'camera-default');
        var parent=a.comp.layers.addNull(); parent.threeDLayer=true;parent.property('ADBE Transform Group').property('ADBE Position').setValue([160,90,0]);
        cam.parent=parent;
        parent.property('ADBE Transform Group').property('ADBE Rotate Y').setValueAtTime(0,0);
        parent.property('ADBE Transform Group').property('ADBE Rotate Y').setValueAtTime(1,90);
        save(a.comp,'camera-parent90',1);
        e.property('Sync AE Position').setValue(1);save(a.comp,'camera-position',0);
        e.property('Sync AE Camera').setValue(0);e.property('Sync AE Position').setValue(0);
        var names=['horizontal','vertical','grid','cross'];
        for(var l=1;l<=4;l++) {for(var projection=2;projection<=3;projection++) {
            var b=fixture('Cube '+l+' '+projection,names[l-1]+'-'+(projection===3?'eac':'cube'),projection,l);
            b.effect.property('Rotation Y').setValue(45);b.effect.property('Rotation X').setValue(-25);
            b.effect.property('Interpolation').setValue(5);
            save(b.comp,'cube-'+l+'-'+projection);
        }}
        for(var cl=1;cl<=4;cl++){
            var ci=footage(names[cl-1]+'-cube');
            var reference=fixture('Cube reference '+cl,names[cl-1]+'-reference',1,1,1,[ci.width,ci.height]);
            reference.effect.property('Rotation Y').setValue(45);reference.effect.property('Rotation X').setValue(-25);reference.effect.property('Interpolation').setValue(5);save(reference.comp,'cube-reference-'+cl);
        }
        app.project.bitsPerChannel=8;
        for(var dl=1;dl<=4;dl++) {for(var dip=2;dip<=3;dip++) {
            var dc=fixture('Cube depth '+dl+' '+dip,names[dl-1]+'-'+(dip===3?'eac':'cube'),dip,dl);
            var dm=dc.comp.layers.add(footage('depth-'+names[dl-1]+'-'+(dip===3?'eac':'cube')));dm.enabled=false;
            dc.effect.property('Depth Map Layer').setValue(dm.index);dc.effect.property('Position X').setValue(500);
            save(dc.comp,'depth-cube-'+dl+'-'+dip);
        }}
        for(var drl=1;drl<=4;drl++){
            var dri=footage(names[drl-1]+'-cube');
            var dr=fixture('Cube depth reference '+drl,names[drl-1]+'-reference',1,1,1,[dri.width,dri.height]);
            var drm=dr.comp.layers.add(footage('depth-split'));drm.enabled=false;
            dr.effect.property('Depth Map Layer').setValue(drm.index);dr.effect.property('Position X').setValue(500);
            save(dr.comp,'depth-cube-reference-'+drl);
        }
        app.project.bitsPerChannel=32;
        var par=fixture('Pixel Aspect','latlong',1,1,2);save(par.comp,'par2');
        var sized=fixture('Sphere size tests','latlong');var se=sized.effect;
        se.property('Field of View').setValue(180);se.property('Orthographic Width').setValue(40000);
        for(var sb=8;sb<=32;sb*=2){app.project.bitsPerChannel=sb;for(var sp=2;sp<=4;sp++){
            se.property('Output Projection').setValue(sp);
            for(var ss=50;ss<=150;ss+=50){se.property('Sphere Scale (%)').setValue(ss);save(sized.comp,'sphere-'+sp+'-'+ss+'-'+sb);}
        }}
        app.project.bitsPerChannel=8;se.property('Output Projection').setValue(2);se.property('Sphere Scale (%)').setValue(50);
        sized.comp.resolutionFactor=[2,2];save(sized.comp,'sphere-half');sized.comp.resolutionFactor=[1,1];
        var animatedSize=se.property('Sphere Scale (%)');animatedSize.setValueAtTime(0,50);animatedSize.setValueAtTime(1,150);
        save(sized.comp,'sphere-animated-50',0);save(sized.comp,'sphere-animated-150',1);
        var sizePar=fixture('Sphere PAR2','latlong',1,1,2);sizePar.effect.property('Output Projection').setValue(2);sizePar.effect.property('Sphere Scale (%)').setValue(50);save(sizePar.comp,'sphere-par2');
        var sizePerspective=fixture('Sphere Perspective','latlong');sizePerspective.effect.property('Sphere Scale (%)').setValue(50);save(sizePerspective.comp,'sphere-perspective');
        record('PASS sphere size renders');
        app.project.bitsPerChannel=32;
        var preview={comp:app.project.items.addComp('CATPREVIEW - Panorama Reframe',512,512,1,2,24)};
        var canvas=preview.comp.layers.add(footage('grid'));canvas.name='Panorama Reframe';
        var pe=canvas.property('ADBE Effect Parade').addProperty('PanoramaReframe');pe.property('Output Projection').setValue(2);pe.property('Field of View').setValue(220);pe.property('Rotation Y').setValue(35);pe.property('Rotation X').setValue(-25);pe.property('Antialiasing').setValue(2);
        pe.property('Position X').setValue(2000);pe.property('Position Z').setValue(1500);
        pe.property('Sphere Scale (%)').setValue(85);
        save(preview.comp,'catalog');
        app.project.save(new File(out.fsName+'/panorama-reframe-tests.aep'));
        record('PASS script completed');
    } catch(error) {record('FAIL '+error.toString()+' line '+error.line);}
    finally {log.close();app.endSuppressDialogs(false);}
})();
