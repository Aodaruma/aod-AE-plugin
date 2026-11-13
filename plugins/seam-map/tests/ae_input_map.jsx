// Run only after the user has authorized AE testing. Never open a user project here.
(function () {
    var root = new Folder($.global.AOD_SEAM_TEST_ROOT || '');
    if (!new File(root.fsName + '/plugins/seam-map/Cargo.toml').exists) {
        throw new Error('Set AOD_SEAM_TEST_ROOT to the current worktree.');
    }
    var owned = false;
    if (app.project.file) {
        var path = app.project.file.fsName.replace(/\\/g, '/').toLowerCase();
        var prefix = root.fsName.replace(/\\/g, '/').toLowerCase() + '/target/';
        owned = path.indexOf(prefix) === 0 && /\/(seam-map-preview|seam-map-input-test)\.aep$/.test(path);
    }
    if (app.project.numItems && !owned) { throw new Error('An empty or saved task-owned test project is required.'); }
    var parent = new Folder(root.fsName + '/target/seam-map-development');
    if (!parent.exists && !parent.create()) { throw new Error('Cannot create output directory.'); }
    var out = new Folder(parent.fsName + '/ae2025');
    if (!out.exists && !out.create()) { throw new Error('Cannot create AE output directory.'); }
    var log = new File(out.fsName + '/result.txt');
    if (!log.open('w')) { throw new Error('Cannot create test log.'); }
    function record(s) { log.writeln(s); log.close(); log.open('a'); }
    var w = 96, h = 64, frame = 1 / 24;
    function comp(name) { return app.project.items.addComp(name, w, h, 1, 1, 24); }
    function queue(c, name) {
        var item = app.project.renderQueue.items.add(c);
        item.applyTemplate('Best Settings'); item.timeSpanStart = 0; item.timeSpanDuration = frame;
        var module = item.outputModule(1); module.applyTemplate('PNG seq (untone-map)');
        module = item.outputModule(1); module.file = new File(out.fsName + '/' + name + '_[#####].png');
    }
    app.beginSuppressDialogs();
    try {
        for (var q = 1; q <= app.project.renderQueue.numItems; q++) {
            if (app.project.renderQueue.item(q).status === RQItemStatus.QUEUED) { app.project.renderQueue.item(q).render = false; }
        }
        var source = comp('SeamMap Input Source');
        source.layers.addSolid([0.1, 0.2, 0.4], 'Background', w, h, 1);
        for (var i = 0; i < 12; i++) {
            var bar = source.layers.addSolid([0.1 + i * 0.06, 0.85 - i * 0.055, 0.2 + i * 0.04], 'Bar ' + i, 4, 16 + (i % 6) * 7, 1);
            bar.position.setValue([4 + i * 8, h / 2]);
        }
        var map = comp('SeamMap Three Levels');
        for (var j = 0; j < 3; j++) {
            var gray = [0, 0.3, 1][j];
            var band = map.layers.addSolid([gray, gray, gray], 'Level ' + gray, 32, h, 1);
            band.position.setValue([j * 32 + 16, h / 2]);
        }
        var cases = [
            ['identity', 100, 100, 30, 0, 0], ['no-map', 50, 150, 30, 0, 1],
            ['black-pin', 50, 150, 0, 0, 0], ['mid-pin', 50, 150, 30, 0, 0],
            ['white-pin', 50, 150, 100, 0, 0], ['flip-pin', -150, 175, 30, 0, 0],
            ['restored-pin', 50, 150, 30, 1, 0],
            ['x-only', 50, 150, 30, 0, 0, 1], ['y-only', 50, 150, 30, 0, 0, 2]
        ];
        var preview = null;
        for (var di = 0; di < 3; di++) {
            var depth = [8, 16, 32][di]; app.project.bitsPerChannel = depth;
            var first = app.project.renderQueue.numItems + 1;
            queue(source, 'source-' + depth); queue(map, 'control-' + depth);
            for (var ci = 0; ci < cases.length; ci++) {
                var s = cases[ci], name = s[0] + '-' + depth;
                var c = comp('SeamMap ' + name);
                var mapLayer = c.layers.add(map); mapLayer.enabled = false;
                var layer = c.layers.add(source);
                var effect = layer.property('ADBE Effect Parade').addProperty('SeamMap');
                if (!s[5]) { effect.property('Scale Map Layer').setValue(mapLayer.index); }
                effect.property('Map Channel').setValue(2);
                effect.property('Black Scale (%)').setValue(s[1]); effect.property('White Scale (%)').setValue(s[2]);
                effect.property('Anchor Map Value (%)').setValue(s[3]);
                effect.property('Restore Original Size').setValue(s[4]);
                if (s[6]) {
                    effect.property('Separate X/Y Scale').setValue(1);
                    effect.property('Black Scale X (%)').setValue(s[6] === 1 ? s[1] : 100);
                    effect.property('White Scale X (%)').setValue(s[6] === 1 ? s[2] : 100);
                    effect.property('Black Scale Y (%)').setValue(s[6] === 2 ? s[1] : 100);
                    effect.property('White Scale Y (%)').setValue(s[6] === 2 ? s[2] : 100);
                }
                queue(c, name);
                if (s[0] === 'mid-pin' && depth === 32) { preview = c; }
            }
            app.project.renderQueue.render();
            for (var ri = first; ri <= app.project.renderQueue.numItems; ri++) {
                if (app.project.renderQueue.item(ri).status !== RQItemStatus.DONE) { throw new Error('Incomplete render at ' + depth + ' bpc.'); }
            }
            record('PASS rendered 11 images at ' + depth + ' bpc');
        }
        app.project.save(new File(out.fsName + '/seam-map-input-test.aep'));
        if (preview) { preview.openInViewer(); preview.layer(1).selected = true; }
        record('PASS AE ' + app.version + ' host renders completed');
    } catch (error) { record('FAIL ' + error.toString() + ' line=' + error.line); throw error; }
    finally { log.close(); app.endSuppressDialogs(false); }
}());
