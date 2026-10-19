// Run only in an authorized, empty AE test project.
(function () {
    var root = new Folder($.global.AOD_SEAM_TEST_ROOT || '');
    if (!new File(root.fsName + '/plugins/seam-scale/Cargo.toml').exists) {
        throw new Error('Set AOD_SEAM_TEST_ROOT to the current worktree.');
    }
    if (app.project.numItems) { throw new Error('An empty test project is required.'); }
    var parent = new Folder(root.fsName + '/target/seam-scale-0.3.0');
    if (!parent.exists && !parent.create()) { throw new Error('Cannot create output directory.'); }
    var out = new Folder(parent.fsName + '/layer-map');
    if (!out.exists && !out.create()) { throw new Error('Cannot create AE output directory.'); }
    var log = new File(out.fsName + '/result.txt');
    if (!log.open('w')) { throw new Error('Cannot create test log.'); }
    function record(s) { log.writeln(s); log.close(); log.open('a'); }
    var w = 96, h = 64;
    function comp(name) { return app.project.items.addComp(name, w, h, 1, 1, 24); }
    function queue(c, name) {
        var item = app.project.renderQueue.items.add(c);
        item.applyTemplate('Best Settings'); item.timeSpanStart = 0; item.timeSpanDuration = 1 / 24;
        var module = item.outputModule(1); module.applyTemplate('PNG seq (untone-map)');
        module = item.outputModule(1); module.file = new File(out.fsName + '/' + name + '_[#####].png');
    }
    function makeMap(name, levels) {
        var c = comp(name);
        for (var j = 0; j < 3; j++) {
            var v = levels[j], band = c.layers.addSolid([v, v, v], 'Level ' + v, 32, h, 1);
            band.position.setValue([j * 32 + 16, h / 2]);
        }
        return c;
    }
    app.beginSuppressDialogs();
    try {
        var source = comp('Layer Map Source');
        source.layers.addSolid([0.1, 0.2, 0.4], 'Background', w, h, 1);
        for (var i = 0; i < 12; i++) {
            var bar = source.layers.addSolid([0.1 + i * 0.06, 0.85 - i * 0.055, 0.2 + i * 0.04], 'Bar ' + i, 4, 16 + (i % 6) * 7, 1);
            bar.position.setValue([4 + i * 8, h / 2]);
        }
        var map = makeMap('Anchor distance map', [0, 0.5, 1]);
        var complement = makeMap('Complement map', [1, 0.5, 0]);
        var cases = ['identity', 'no-map', 'distance', 'complement', 'flip', 'restored', 'x-only', 'y-only', 'map-off', 'normal'];
        var preview = null;
        for (var di = 0; di < 3; di++) {
            var depth = [8, 16, 32][di]; app.project.bitsPerChannel = depth;
            var first = app.project.renderQueue.numItems + 1;
            queue(source, 'source-' + depth); queue(map, 'control-' + depth);
            for (var ci = 0; ci < cases.length; ci++) {
                var name = cases[ci], c = comp('Layer Map ' + name + '-' + depth);
                var mapLayer = c.layers.add(name === 'complement' ? complement : map); mapLayer.enabled = false;
                var layer = c.layers.add(source);
                var e = layer.property('ADBE Effect Parade').addProperty('SeamScale');
                if (e.property('Use Map Layer').value !== 0) { throw new Error('Expected map OFF by default.'); }
                e.property('Scale (%)').setValue(75);
                if (name !== 'normal') {
                    e.property('Use Map Layer').setValue(name === 'map-off' ? 0 : 1);
                    if (name !== 'no-map') { e.property('Scale Map Layer').setValue(mapLayer.index); }
                    e.property('Map Channel').setValue(2);
                    // Avoid nested conditional expressions: ExtendScript groups them differently.
                    var minimum = 75, maximum = 150;
                    if (name === 'identity') { minimum = 100; maximum = 100; }
                    else if (name === 'flip') { minimum = -75; maximum = -150; }
                    e.property('Min Scale (%)').setValue(minimum);
                    e.property('Max Scale (%)').setValue(maximum);
                    e.property('Anchor Map Value (%)').setValue(50);
                    e.property('Restore Original Size').setValue(name === 'restored' ? 1 : 0);
                    if (name === 'x-only' || name === 'y-only') {
                        e.property('Separate X/Y Scale').setValue(1);
                        e.property('Min Scale X (%)').setValue(name === 'x-only' ? 75 : 100);
                        e.property('Max Scale X (%)').setValue(name === 'x-only' ? 150 : 100);
                        e.property('Min Scale Y (%)').setValue(name === 'y-only' ? 75 : 100);
                        e.property('Max Scale Y (%)').setValue(name === 'y-only' ? 150 : 100);
                    }
                }
                queue(c, name + '-' + depth);
                if (name === 'distance' && depth === 32) { preview = c; }
            }
            app.project.renderQueue.render();
            for (var ri = first; ri <= app.project.renderQueue.numItems; ri++) {
                if (app.project.renderQueue.item(ri).status !== RQItemStatus.DONE) { throw new Error('Incomplete render at ' + depth + ' bpc.'); }
            }
            record('PASS rendered 12 images at ' + depth + ' bpc');
        }
        app.project.save(new File(out.fsName + '/seam-scale-layer-map.aep'));
        if (preview) {
            preview.openInViewer();
            for (var li = 1; li <= preview.numLayers; li++) { preview.layer(li).selected = false; }
            preview.layer(1).selected = true;
            preview.layer(1).property('ADBE Effect Parade').property(1).selected = true;
        }
        record('PASS AE ' + app.version + ' host renders completed');
    } catch (error) { record('FAIL ' + error.toString() + ' line=' + error.line); throw error; }
    finally { log.close(); app.endSuppressDialogs(false); }
}());
