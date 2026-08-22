(function scatterMapNextSmokeTest() {
    var invocationFile = new File($.fileName);
    var outputBase = invocationFile.name === "ae_smoke_test.jsx"
        ? invocationFile.parent
        : Folder.temp;
    var outputDir = new Folder(outputBase.fsName + "/ae_smoke_output");
    if (!outputDir.exists && !outputDir.create()) {
        throw new Error("Could not create output folder: " + outputDir.fsName);
    }

    var runId = String(new Date().getTime());
    var logFile = new File(outputDir.fsName + "/scatter_map_next_" + runId + ".log");
    logFile.encoding = "UTF-8";
    if (!logFile.open("w")) {
        throw new Error("Could not open log file: " + logFile.fsName);
    }

    function log(message) {
        logFile.writeln(message);
    }

    function setValue(effect, name, value) {
        var property = effect.property(name);
        if (!property) {
            for (var index = 1; index <= effect.numProperties; index += 1) {
                var candidate = effect.property(index);
                if (candidate.name && name.indexOf(candidate.name) === 0) {
                    property = candidate;
                    break;
                }
            }
        }
        if (!property) {
            throw new Error("Missing parameter: " + name);
        }
        property.setValue(value);
    }

    function waitForSavedFile(path, timeoutMs) {
        var deadline = new Date().getTime() + timeoutMs;
        while (new Date().getTime() < deadline) {
            var file = new File(path);
            if (file.exists && file.length > 0) {
                return file;
            }
            $.sleep(25);
        }
        return new File(path);
    }

    var originalBpc = app.project.bitsPerChannel;
    var originalActiveItem = app.project.activeItem;
    var createdItems = [];
    var testComp = null;
    var sourceComp = null;
    var finalResult = "FAIL";

    app.beginUndoGroup("ScatterMapNext smoke test");
    try {
        var width = 640;
        var height = 360;
        var duration = 1;
        var frameRate = 30;
        var columns = 8;
        var rows = 5;
        var cellWidth = Math.ceil(width / columns);
        var cellHeight = Math.ceil(height / rows);

        sourceComp = app.project.items.addComp(
            "__ScatterMapNext_Source_" + runId,
            width,
            height,
            1,
            duration,
            frameRate
        );
        createdItems.push(sourceComp);

        for (var y = 0; y < rows; y += 1) {
            for (var x = 0; x < columns; x += 1) {
                var red = ((x * 37 + y * 11) % 101) / 100;
                var green = ((x * 13 + y * 47 + 19) % 101) / 100;
                var blue = ((x * 61 + y * 23 + 7) % 101) / 100;
                var layer = sourceComp.layers.addSolid(
                    [red, green, blue],
                    "cell_" + x + "_" + y,
                    cellWidth,
                    cellHeight,
                    1,
                    duration
                );
                createdItems.push(layer.source);
                layer.property("ADBE Transform Group").property("ADBE Position").setValue([
                    x * cellWidth + cellWidth / 2,
                    y * cellHeight + cellHeight / 2
                ]);
            }
        }

        testComp = app.project.items.addComp(
            "__ScatterMapNext_Smoke_" + runId,
            width,
            height,
            1,
            duration,
            frameRate
        );
        var testLayer = testComp.layers.add(sourceComp);
        var mapLayer = testComp.layers.addSolid(
            [0.75, 0.5, 1],
            "__ScatterMapNext_Map_" + runId,
            width,
            height,
            1,
            duration
        );
        createdItems.push(mapLayer.source);
        createdItems.push(testComp);
        mapLayer.enabled = false;
        var effect = testLayer.property("ADBE Effect Parade").addProperty("ScatterMapNext");
        if (!effect) {
            throw new Error("ScatterMapNext effect was not found");
        }

        log("effect=" + effect.name + " matchName=" + effect.matchName);
        for (var p = 1; p <= effect.numProperties; p += 1) {
            log("param[" + p + "]=" + effect.property(p).name);
        }

        setValue(effect, "Amount (%)", 100);
        setValue(effect, "Radius (px)", 80);
        setValue(effect, "Grain Size (px)", 24);
        setValue(effect, "Gather Samples", 4);
        setValue(effect, "Direction (deg)", 25);
        setValue(effect, "Anisotropy (%)", 55);
        setValue(effect, "Shape", 2);
        setValue(effect, "Grain Size Randomness (%)", 30);
        setValue(effect, "Position Randomness (%)", 100);
        setValue(effect, "Grain Density (%)", 400);
        setValue(effect, "Shape Randomness (%)", 20);
        setValue(effect, "Fill", 1);
        setValue(effect, "Seed", 73);
        setValue(effect, "Map Amount", 1);
        setValue(effect, "Amount Map Layer (None = Input)", mapLayer.index);
        setValue(effect, "Map Radius", 1);
        setValue(effect, "Radius Map Layer (None = Input)", mapLayer.index);
        setValue(effect, "Grain Size Min (px)", 8);
        setValue(effect, "Grain Size Max (px)", 24);
        setValue(effect, "Map Grain Size", 1);
        setValue(effect, "Grain Size Map Layer (None = Input)", mapLayer.index);
        setValue(effect, "Map Anisotropy", 1);
        setValue(effect, "Anisotropy Map Layer (None = Input)", mapLayer.index);
        setValue(effect, "Divergence Source", 1);

        var modes = [
            { name: "gather", value: 1 },
            { name: "swap", value: 2 }
        ];
        var depths = [8, 16, 32];
        for (var baselineDepth = 0; baselineDepth < depths.length; baselineDepth += 1) {
            app.project.bitsPerChannel = depths[baselineDepth];
            var baselineOutput = new File(
                outputDir.fsName +
                "/scatter_map_next_baseline_" + depths[baselineDepth] + "bpc_" + runId + ".png"
            );
            var ignoredBaselineTimerValue = $.hiresTimer;
            sourceComp.saveFrameToPng(0, baselineOutput);
            baselineOutput = waitForSavedFile(baselineOutput.fsName, 5000);
            var baselineElapsedMs = $.hiresTimer / 1000;
            if (!baselineOutput.exists || baselineOutput.length <= 0) {
                throw new Error("Baseline frame was not saved: " + baselineOutput.fsName);
            }
            log(
                "baseline " + depths[baselineDepth] +
                "bpc ok=" + baselineOutput.exists +
                " bytes=" + (baselineOutput.exists ? baselineOutput.length : 0) +
                " elapsed_ms=" + baselineElapsedMs.toFixed(2)
            );
        }
        setValue(effect, "Amount (%)", 0);
        for (var passThroughDepth = 0; passThroughDepth < depths.length; passThroughDepth += 1) {
            app.project.bitsPerChannel = depths[passThroughDepth];
            var passThroughOutput = new File(
                outputDir.fsName +
                "/scatter_map_next_passthrough_" + depths[passThroughDepth] + "bpc_" + runId + ".png"
            );
            var ignoredPassThroughTimerValue = $.hiresTimer;
            testComp.saveFrameToPng(0, passThroughOutput);
            passThroughOutput = waitForSavedFile(passThroughOutput.fsName, 5000);
            var passThroughElapsedMs = $.hiresTimer / 1000;
            if (!passThroughOutput.exists || passThroughOutput.length <= 0) {
                throw new Error("Pass-through frame was not saved: " + passThroughOutput.fsName);
            }
            log(
                "passthrough " + depths[passThroughDepth] +
                "bpc ok=" + passThroughOutput.exists +
                " bytes=" + (passThroughOutput.exists ? passThroughOutput.length : 0) +
                " elapsed_ms=" + passThroughElapsedMs.toFixed(2)
            );
        }
        setValue(effect, "Amount (%)", 100);
        for (var m = 0; m < modes.length; m += 1) {
            setValue(effect, "Scatter Mode", modes[m].value);
            for (var d = 0; d < depths.length; d += 1) {
                app.project.bitsPerChannel = depths[d];
                setValue(effect, "Seed", 73 + m * 10);
                var outputFile = new File(
                    outputDir.fsName +
                    "/scatter_map_next_" + modes[m].name + "_" + depths[d] + "bpc_" + runId + ".png"
                );
                var ignoredTimerValue = $.hiresTimer;
                testComp.saveFrameToPng(0, outputFile);
                outputFile = waitForSavedFile(outputFile.fsName, 5000);
                var elapsedMs = $.hiresTimer / 1000;
                if (!outputFile.exists || outputFile.length <= 0) {
                    throw new Error("Frame was not saved: " + outputFile.fsName);
                }
                log(
                    modes[m].name +
                    " " + depths[d] +
                    "bpc ok=" + outputFile.exists +
                    " bytes=" + (outputFile.exists ? outputFile.length : 0) +
                    " elapsed_ms=" + elapsedMs.toFixed(2)
                );
            }
        }

        setValue(effect, "Scatter Mode", 1);
        setValue(effect, "Shape", 3);
        setValue(effect, "Kernel Texture (None = Input)", mapLayer.index);
        app.project.bitsPerChannel = 32;
        var textureOutput = new File(
            outputDir.fsName + "/scatter_map_next_texture_32bpc_" + runId + ".png"
        );
        var ignoredTextureTimerValue = $.hiresTimer;
        testComp.saveFrameToPng(0, textureOutput);
        textureOutput = waitForSavedFile(textureOutput.fsName, 5000);
        var textureElapsedMs = $.hiresTimer / 1000;
        if (!textureOutput.exists || textureOutput.length <= 0) {
            throw new Error("Frame was not saved: " + textureOutput.fsName);
        }
        log(
            "texture 32bpc ok=" + textureOutput.exists +
            " bytes=" + (textureOutput.exists ? textureOutput.length : 0) +
            " elapsed_ms=" + textureElapsedMs.toFixed(2)
        );

        setValue(effect, "Shape", 2);
        setValue(effect, "Anisotropy Map Layer (None = Input)", 0);
        var divergenceModes = [
            { name: "divergence_direction", value: 4 },
            { name: "divergence_rotation", value: 5 }
        ];
        for (var divergenceIndex = 0; divergenceIndex < divergenceModes.length; divergenceIndex += 1) {
            setValue(effect, "Anisotropy Map Mode", divergenceModes[divergenceIndex].value);
            var divergenceOutput = new File(
                outputDir.fsName + "/scatter_map_next_" +
                divergenceModes[divergenceIndex].name + "_32bpc_" + runId + ".png"
            );
            var ignoredDivergenceTimerValue = $.hiresTimer;
            testComp.saveFrameToPng(0, divergenceOutput);
            divergenceOutput = waitForSavedFile(divergenceOutput.fsName, 5000);
            var divergenceElapsedMs = $.hiresTimer / 1000;
            if (!divergenceOutput.exists || divergenceOutput.length <= 0) {
                throw new Error("Frame was not saved: " + divergenceOutput.fsName);
            }
            log(
                divergenceModes[divergenceIndex].name +
                " 32bpc ok=" + divergenceOutput.exists +
                " bytes=" + (divergenceOutput.exists ? divergenceOutput.length : 0) +
                " elapsed_ms=" + divergenceElapsedMs.toFixed(2)
            );
        }
        log("RESULT=PASS");
        finalResult = "PASS";
    } catch (error) {
        log("RESULT=FAIL");
        log("ERROR=" + error.toString());
        if (error.line) {
            log("LINE=" + error.line);
        }
    } finally {
        app.project.bitsPerChannel = originalBpc;
        for (var i = createdItems.length - 1; i >= 0; i -= 1) {
            try {
                createdItems[i].remove();
            } catch (cleanupError) {
                log("CLEANUP_WARNING=" + cleanupError.toString());
            }
        }
        try {
            if (originalActiveItem && originalActiveItem.openInViewer) {
                originalActiveItem.openInViewer();
            }
        } catch (restoreError) {
            log("RESTORE_WARNING=" + restoreError.toString());
        }
        app.endUndoGroup();
        logFile.close();
    }
    return JSON.stringify({
        result: finalResult,
        outputDir: outputDir.fsName,
        logFile: logFile.fsName
    });
}());
