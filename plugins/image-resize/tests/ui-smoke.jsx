// Run in a new AE process with UI: AfterFX -m -r <this file>.
(function () {
    if (app.project && (app.project.numItems > 0 || app.project.file)) {
        throw new Error("UI smoke test requires an empty project");
    }
    var root = File($.fileName).parent.parent.parent.parent;
    var comp = app.project.items.addComp("ImageResize UI", 320, 180, 1, 1, 30);
    var layer = comp.layers.addSolid([0.2, 0.6, 0.9], "ImageResize UI", 160, 90, 1, 1);
    layer.property("ADBE Effect Parade").addProperty("ImageResize");
    layer.selected = true;
    comp.openInViewer();
    app.project.save(new File(root.fsName + "/target/ae-smoke/image-resize-ui.aep"));
    var command = app.findMenuCommandId("Effect Controls");
    if (command) { app.executeCommand(command); }
}());
