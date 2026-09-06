(function () {
    var environmentPhase = $.getenv("AOD_CATALOG_PHASE");
    var phase = (typeof AOD_CATALOG_PHASE !== "undefined") ? AOD_CATALOG_PHASE : (environmentPhase || "build");
    var headless = $.getenv("AOD_CATALOG_HEADLESS") === "1";
    var repoRoot = new Folder(File($.fileName).parent.parent.parent.fsName);
    var docsRoot = new Folder(repoRoot.fsName + "/docs/catalog");
    var previewRoot = new Folder(docsRoot.fsName + "/assets/previews");
    var overviewRoot = new Folder(docsRoot.fsName + "/assets/overview");
    var projectRoot = new Folder(repoRoot.fsName + "/catalog/ae2025-plugin-catalog");
    var reportFile = new File(projectRoot.fsName + "/build-report.txt");
    var projectFile = new File(projectRoot.fsName + "/aod-plugin-catalog-ae2025.aep");
    var seedProjectPath = $.getenv("AOD_CATALOG_SEED_PROJECT");
    var log = [];
    var warnings = [];

    function notify(message) {
        if (headless) {
            $.writeln(message);
        } else {
            alert(message);
        }
    }

    var catalog = [
        { name: "Color Ajust", aod: "AOD_ColorAjust", slug: "color-ajust", category: "P", match: "ColorAjust" },
        { name: "Color Composite", aod: "AOD_ColorComposite", slug: "color-composite", category: "P", comp: "03 Color Composite" },
        { name: "Color Convert", aod: "AOD_ColorConvert", slug: "color-convert", category: "P", match: "ColorConvert" },
        { name: "Color Quantize", aod: "AOD_ColorQuantize", slug: "color-quantize", category: "P", comp: "04 Color Quantize" },
        { name: "Displace Scatter", aod: "AOD_DisplaceScatter", slug: "displace-scatter", category: "P", comp: "07 Displace Scatter" },
        { name: "FFT", aod: "AOD_FFT", slug: "fft", category: "P", comp: "09 FFT" },
        { name: "Fourier Filter", aod: "AOD_FourierFilter", slug: "fourier-filter", category: "P", comp: "10 Fourier Filter" },
        { name: "Gradient Blur", aod: "AOD_GradientBlur", slug: "gradient-blur", category: "P", comp: "12 Gradient Blur" },
        { name: "Gradient Displace", aod: "AOD_GradientDisplace", slug: "gradient-displace", category: "P", comp: "13 Gradient Displace" },
        { name: "Glass Displace", aod: "AOD_GlassDisplace", slug: "glass-displace", category: "P", match: "GlassDisplace", fresh: true },
        { name: "Image Calculate", aod: "AOD_ImageCalculate", slug: "image-calculate", category: "P", match: "ImageCalculate" },
        { name: "Image Crypt", aod: "AOD_ImageCrypt", slug: "image-encrypt-decrypt", category: "P", match: "ImageEncryptDecrypt", fresh: true },
        { name: "Image Scaler", aod: "AOD_ImageScaler", slug: "image-scaler", category: "P", comp: "15 Image Scaler" },
        { name: "Image Transform", aod: "AOD_ImageTransform", slug: "image-transform", category: "P", match: "ImageTransform", fresh: true },
        { name: "Mobius Transform", aod: "AOD_MobiusTransform", slug: "mobius-transform", category: "P", match: "MobiusTransform" },
        { name: "Scatter Map", aod: "AOD_ScatterMap", slug: "scatter-map", category: "P", comp: "19 Scatter Map" },
        { name: "Singular Value Decompose", aod: "AOD_SingularValueDecompose", slug: "singular-value-decompose", category: "P", comp: "20 Singular Value Decompose" },

        { name: "Color Change", aod: "AOD_ColorChange", slug: "color-change", category: "A", match: "ColorChange" },
        { name: "Color Boundary Blur", aod: "AOD_ColorBoundaryBlur", slug: "color-boundary-blur", category: "A", match: "ColorBoundaryBlur" },
        { name: "Color Select", aod: "AOD_ColorSelect", slug: "color-select", category: "A", comp: "05 Color Select" },
        { name: "Contour Generate", aod: "AOD_ContourGenerate", slug: "contour-generate", category: "A", match: "ContourGenerate" },
        { name: "Differential Generate", aod: "AOD_DifferentialGenerate", slug: "differential-generate", category: "A", match: "DifferentialGenerate" },
        { name: "Distance Generate", aod: "AOD_DistanceGenerate", slug: "distance-generate", category: "A", match: "DistanceGenerate" },
        { name: "Eyedropper Mask", aod: "AOD_EyedropperMask", slug: "eyedropper-mask", category: "A", comp: "08 Eyedropper Mask" },
        { name: "Image Relight", aod: "AOD_ImageRelight", slug: "image-relight", category: "A", match: "ImageRelight", fresh: true },
        { name: "Light Wrap", aod: "AOD_LightWrap", slug: "light-wrap", category: "A", comp: "16 Light Wrap" },
        { name: "Line Repaint", aod: "AOD_LineRepaint", slug: "line-repaint", category: "A", comp: "17 Line Repaint" },
        { name: "Normal Generate", aod: "AOD_NormalGenerate", slug: "normal-generate", category: "A", match: "NormalGenerate" },
        { name: "Pixel Extend", aod: "AOD_PixelExtend", slug: "pixel-extend", category: "A", comp: "18 Pixel Extend" },
        { name: "Region Colorize", aod: "AOD_RegionColorize", slug: "region-colorize", category: "A", match: "RegionColorize" },

        { name: "Channel Remap", aod: "AOD_ChannelRemap", slug: "channel-remap", category: "D", comp: "01 Channel Remap" },
        { name: "Depth Fog", aod: "AOD_DepthFog", slug: "depth-fog", category: "D", comp: "06 Depth Fog" },
        { name: "IFFT", aod: "AOD_IFFT", slug: "ifft", category: "D", comp: "14 IFFT" },
        { name: "Space Convert", aod: "AOD_SpaceConvert", slug: "space-convert", category: "D", match: "SpaceConvert", fresh: true },

        { name: "Checker Generate", aod: "AOD_CheckerGenerate", slug: "checker-generate", category: "G", comp: "02 Checker Generate" },
        { name: "Gabor Generate", aod: "AOD_GaborGenerate", slug: "gabor-generate", category: "G", comp: "11 Gabor Generate" },
        { name: "Rainbow Generate", aod: "AOD_RainbowGenerate", slug: "rainbow-generate", category: "G", match: "RainbowGenerate", fresh: true },
        { name: "Voronoi Generate", aod: "AOD_VoronoiGenerate", slug: "voronoi-generate", category: "G", match: "VoronoiGenerate" }
    ];

    for (var catalogIndex = 0; catalogIndex < catalog.length; catalogIndex++) {
        if (!catalog[catalogIndex].comp && catalog[catalogIndex].match) {
            catalog[catalogIndex].comp = "CATPREVIEW - " + catalog[catalogIndex].name;
        }
    }

    function ensureFolder(folder) {
        if (!folder.exists && !folder.create()) {
            throw new Error("Could not create folder: " + folder.fsName);
        }
    }

    function findComp(name) {
        for (var i = 1; i <= app.project.numItems; i++) {
            var item = app.project.item(i);
            if (item instanceof CompItem && item.name === name) {
                return item;
            }
        }
        return null;
    }

    function removeCompsWithPrefix(prefix) {
        for (var i = app.project.numItems; i >= 1; i--) {
            var item = app.project.item(i);
            if (item instanceof CompItem && item.name.indexOf(prefix) === 0) {
                item.remove();
            }
        }
    }

    function setProperty(effect, name, value) {
        try {
            var prop = effect.property(name);
            if (!prop) {
                warnings.push(effect.name + ": parameter not found: " + name);
                return;
            }
            prop.setValue(value);
        } catch (error) {
            warnings.push(effect.name + ": could not set " + name + " (" + error.toString() + ")");
        }
    }

    function addEffect(layer, matchName) {
        var effects = layer.property("ADBE Effect Parade");
        var effect = effects.addProperty(matchName);
        if (!effect) {
            throw new Error("Effect unavailable: " + matchName);
        }
        return effect;
    }

    function addMaskedSolid(comp, name, color, vertices, opacity) {
        var layer = comp.layers.addSolid(color, name, comp.width, comp.height, 1, comp.duration);
        var mask = layer.property("ADBE Mask Parade").addProperty("ADBE Mask Atom");
        var shape = new Shape();
        shape.vertices = vertices;
        shape.inTangents = [];
        shape.outTangents = [];
        for (var i = 0; i < vertices.length; i++) {
            shape.inTangents.push([0, 0]);
            shape.outTangents.push([0, 0]);
        }
        shape.closed = true;
        mask.property("ADBE Mask Shape").setValue(shape);
        layer.property("ADBE Transform Group").property("ADBE Opacity").setValue(opacity);
        return layer;
    }

    function createFallbackSourceComp() {
        var comp = app.project.items.addComp("Cat Crop 512x512", 512, 512, 1, 1, 30);
        comp.bgColor = [0.025, 0.035, 0.065];
        comp.layers.addSolid([0.025, 0.035, 0.065], "Procedural Background", 512, 512, 1, comp.duration);
        addMaskedSolid(comp, "Warm Diagonal", [0.95, 0.25, 0.08], [[-40, 390], [385, -35], [555, 135], [130, 560]], 82);
        addMaskedSolid(comp, "Cool Plane", [0.02, 0.48, 0.95], [[-30, 80], [275, 25], [390, 255], [60, 335]], 76);
        addMaskedSolid(comp, "Green Diamond", [0.08, 0.86, 0.48], [[350, 115], [485, 250], [350, 385], [215, 250]], 88);

        var circle = [];
        for (var i = 0; i < 32; i++) {
            var angle = Math.PI * 2 * i / 32;
            circle.push([174 + Math.cos(angle) * 104, 330 + Math.sin(angle) * 104]);
        }
        addMaskedSolid(comp, "Soft Circle", [1.0, 0.82, 0.16], circle, 92);
        addMaskedSolid(comp, "Highlight", [0.96, 0.98, 1.0], [[210, 110], [315, 95], [330, 165], [225, 180]], 86);
        log.push("CREATED procedural fallback source: " + comp.name);
        return comp;
    }

    function addCatLayer(comp, preparedForAnime) {
        var cat = findComp("Cat Crop 512x512");
        if (!cat) {
            cat = createFallbackSourceComp();
        }
        comp.layers.addSolid([0.035, 0.045, 0.065], "Preview Background", 512, 512, 1, comp.duration);
        var layer = comp.layers.add(cat);
        layer.name = preparedForAnime ? "Cel-style Cat Source" : "Photo Cat Source";
        if (preparedForAnime) {
            var quantize = addEffect(layer, "ColorQuantize");
            setProperty(quantize, "Colors (K)", 5);
            setProperty(quantize, "Color Space", 3);
            setProperty(quantize, "Max Iterations", 18);
            setProperty(quantize, "RGB Only", 1);
        }
        return layer;
    }

    function configureOldEffect(item, comp, layer, effect) {
        if (item.match === "ColorAjust") {
            setProperty(effect, "Color Space", 1);
            setProperty(effect, "Hue Shift (deg)", 42);
            setProperty(effect, "Chroma Scale", 1.45);
            setProperty(effect, "Lightness Delta", 0.035);
        } else if (item.match === "ColorChange") {
            setProperty(effect, "Tolerance", 0.24);
            setProperty(effect, "Number of Colors", 1);
            setProperty(effect, "Color1 From", [0.34, 0.30, 0.28, 1]);
            setProperty(effect, "Color1 To", [1.0, 0.24, 0.08, 1]);
        } else if (item.match === "ColorBoundaryBlur") {
            setProperty(effect, "Color Tolerance (%)", 18);
            setProperty(effect, "Color Proximity (px)", 3);
            setProperty(effect, "Number of Colors", 4);
            setProperty(effect, "Color 1", [0.10, 0.08, 0.07, 1]);
            setProperty(effect, "Color 2", [0.38, 0.31, 0.27, 1]);
            setProperty(effect, "Color 3", [0.72, 0.60, 0.49, 1]);
            setProperty(effect, "Color 4", [0.92, 0.84, 0.70, 1]);
            setProperty(effect, "Blur Mode", 3);
            setProperty(effect, "Blur Radius (px)", 12);
            setProperty(effect, "Normal Samples", 25);
            setProperty(effect, "Along-Boundary Radius (%)", 12);
            setProperty(effect, "Post Smooth (px)", 1.5);
            setProperty(effect, "Boundary Width (px)", 3);
            setProperty(effect, "Mask Feather (px)", 3);
        } else if (item.match === "ColorConvert") {
            setProperty(effect, "From Color Space", 1);
            setProperty(effect, "To Color Space", 3);
            setProperty(effect, "Clamp Output 0..1", 1);
        } else if (item.match === "ContourGenerate") {
            setProperty(effect, "Low Threshold", 0.045);
            setProperty(effect, "High Threshold", 0.16);
            setProperty(effect, "Pre Blur Sigma", 1.1);
            setProperty(effect, "Line Width (px)", 3.5);
            setProperty(effect, "Thin Lines (1px)", 0);
            setProperty(effect, "Line Color", [1.0, 0.78, 0.28, 1]);
            setProperty(effect, "Use Alpha", 0);
        } else if (item.match === "DifferentialGenerate") {
            setProperty(effect, "Axis", 3);
            setProperty(effect, "Offset", 0.16);
            setProperty(effect, "Scale", 5.0);
            setProperty(effect, "Edge Mode", 2);
            setProperty(effect, "RGB Only (Keep Alpha)", 1);
        } else if (item.match === "DistanceGenerate") {
            setProperty(effect, "Distance Type", 2);
            setProperty(effect, "Direction", 1);
            setProperty(effect, "Gradient Width (px)", 72);
            setProperty(effect, "Offset", 0.04);
            setProperty(effect, "Label Tolerance", 0.035);
            setProperty(effect, "Use Original Alpha", 0);
        } else if (item.match === "GlassDisplace") {
            setProperty(effect, "Height Source", 1);
            setProperty(effect, "Shape", 7);
            setProperty(effect, "Size / Impact Diameter (px)", 390.0);
            setProperty(effect, "Cell Size (px)", 54.0);
            setProperty(effect, "Facet Relief (%)", 100.0);
            setProperty(effect, "Cell Irregularity (%)", 72.0);
            setProperty(effect, "Crack Width (px)", 2.2);
            setProperty(effect, "Crack Depth (%)", 94.0);
            setProperty(effect, "Radial Cracks", 22);
            setProperty(effect, "Branching (%)", 68.0);
            setProperty(effect, "Crack Jitter (%)", 52.0);
            setProperty(effect, "Stress Rings", 7);
            setProperty(effect, "Normal Strength", 132.0);
            setProperty(effect, "Normal Radius (px)", 1.35);
            setProperty(effect, "Refraction (px)", 58.0);
            setProperty(effect, "Chromatic Dispersion (px)", 6.0);
            setProperty(effect, "Spectral Steps (Manual)", 12);
            setProperty(effect, "Auto Spectral Steps", 1);
            setProperty(effect, "Sampling", 2);
            setProperty(effect, "Edge", 2);
        } else if (item.match === "ImageCalculate") {
            setProperty(effect, "Operation", 3);
            setProperty(effect, "Input B (Operand)", 1);
            setProperty(effect, "Value B (Operand)", 1.75);
            setProperty(effect, "Clamp Result 0..1", 1);
            setProperty(effect, "Use Original Alpha", 1);
        } else if (item.match === "ImageEncryptDecrypt") {
            setProperty(effect, "Operation", 1);
            setProperty(effect, "Algorithm", 7);
            setProperty(effect, "Key A", 48131);
            setProperty(effect, "Key B", 9277);
            setProperty(effect, "Key C", 31847);
            setProperty(effect, "Key D", 60101);
            setProperty(effect, "Rounds", 7);
            setProperty(effect, "Cipher Precision", 1);
            setProperty(effect, "Channels", 1);
        } else if (item.match === "ImageRelight") {
            setProperty(effect, "Source", 1);
            setProperty(effect, "Method", 1);
            setProperty(effect, "Color Tolerance", 0.035);
            setProperty(effect, "Alpha Threshold", 0.01);
            setProperty(effect, "Region Radius (px)", 44.0);
            setProperty(effect, "Height Shape", 1.6);
            setProperty(effect, "Edge Softness (px)", 1.5);
            setProperty(effect, "Surface Solver", 2);
            setProperty(effect, "Boundary Condition", 2);
            setProperty(effect, "Iterations", 128);
            setProperty(effect, "Divergence / Curvature", 2.8);
            setProperty(effect, "Screened Damping", 0.035);
            setProperty(effect, "Edge Feather (px)", 7.0);
            setProperty(effect, "Normal Strength", 7.5);
            setProperty(effect, "Type", 1);
            setProperty(effect, "Color", [1.0, 0.93, 0.82, 1]);
            setProperty(effect, "Intensity", 1.65);
            setProperty(effect, "Azimuth", -38.0);
            setProperty(effect, "Elevation", 32.0);
            setProperty(effect, "Surface", 1);
            setProperty(effect, "Metallic", 0.12);
            setProperty(effect, "Roughness", 0.28);
            setProperty(effect, "IOR", 1.5);
            setProperty(effect, "Specular IOR Level", 0.72);
            setProperty(effect, "Environment Strength", 0.16);
            setProperty(effect, "Exposure (EV)", -0.15);
            setProperty(effect, "Output", 1);
        } else if (item.match === "ImageTransform") {
            setProperty(effect, "Separate Dimensions", 1);
            setProperty(effect, "Scale X", 136.0);
            setProperty(effect, "Scale Y", 82.0);
            setProperty(effect, "Rotation", 18.0);
            setProperty(effect, "Skew", 15.0);
            setProperty(effect, "Skew Axis", -22.0);
            setProperty(effect, "Interpolation", 5);
            setProperty(effect, "Lanczos Lobes", 3.0);
            setProperty(effect, "Sample Outside Image", 1);
            setProperty(effect, "Outside Pixels", 3);
        } else if (item.match === "MobiusTransform") {
            setProperty(effect, "b.re", 0.18);
            setProperty(effect, "b.im", -0.08);
            setProperty(effect, "c.re", 0.34);
            setProperty(effect, "c.im", 0.16);
            setProperty(effect, "d.re", 1.0);
            setProperty(effect, "Edge", 2);
            setProperty(effect, "Interpolation", 2);
            setProperty(effect, "Anti-alias", 3);
        } else if (item.match === "SpaceConvert") {
            setProperty(effect, "Space", 2);
            setProperty(effect, "Direction", 1);
            setProperty(effect, "Angle Offset", -90.0);
            setProperty(effect, "Interpolation", 2);
            setProperty(effect, "Edge", 2);
            setProperty(effect, "Separate Dimensions", 1);
            setProperty(effect, "Scale X", 112.0);
            setProperty(effect, "Scale Y", 92.0);
            setProperty(effect, "Rotation", 9.0);
            setProperty(effect, "Skew", -8.0);
        } else if (item.match === "NormalGenerate") {
            setProperty(effect, "Method", 2);
            setProperty(effect, "Normal Strength", 7.5);
            setProperty(effect, "Label Tolerance", 0.035);
            setProperty(effect, "Edge Softness (px)", 2.0);
            setProperty(effect, "SDF Radius (px)", 52);
            setProperty(effect, "Use Original Alpha", 0);
        } else if (item.match === "RegionColorize") {
            setProperty(effect, "Region Source", 2);
            setProperty(effect, "Tolerance", 0.035);
            setProperty(effect, "Mode", 1);
            setProperty(effect, "Seed", 73);
            setProperty(effect, "Use Original Alpha", 1);
        } else if (item.match === "RainbowGenerate") {
            setProperty(effect, "Shape", 9);
            setProperty(effect, "Coordinates", 2);
            setProperty(effect, "Aspect (-Vertical/+Horizontal)", 0.18);
            setProperty(effect, "Skew", 12.0);
            setProperty(effect, "Ray Count", 18.0);
            setProperty(effect, "Hue Scale (%)", 175.0);
            setProperty(effect, "Hue Offset (deg)", -18.0);
            setProperty(effect, "Chroma Scale (%)", -25.0);
            setProperty(effect, "Chroma Offset (%)", 115.0);
            setProperty(effect, "Lightness Scale (%)", -12.0);
            setProperty(effect, "Lightness Offset (%)", 82.0);
            setProperty(effect, "Color Model", 1);
            setProperty(effect, "Extend", 2);
            setProperty(effect, "Preset", 6);
            setProperty(effect, "Mix (%)", 100.0);
        } else if (item.match === "VoronoiGenerate") {
            setProperty(effect, "Output", 1);
            setProperty(effect, "Mode", 1);
            setProperty(effect, "Cell Size (px)", 82);
            setProperty(effect, "Scale X", 1.0);
            setProperty(effect, "Scale Y", 1.0);
            setProperty(effect, "Randomness", 0.88);
            setProperty(effect, "Seed", 19);
            setProperty(effect, "Smoothness", 0.12);
            setProperty(effect, "Blend Mode", 1);
            setProperty(effect, "Blend Opacity", 1.0);
        }
    }

    function createOldPreview(item) {
        var name = "CATPREVIEW - " + item.name;
        var comp = app.project.items.addComp(name, 512, 512, 1, 1, 30);
        var layer;
        if (item.category === "G") {
            layer = comp.layers.addSolid([0.035, 0.045, 0.065], "Generator Canvas", 512, 512, 1, comp.duration);
        } else {
            layer = addCatLayer(comp, item.category === "A");
        }
        var effect = addEffect(layer, item.match);
        effect.name = item.aod;
        configureOldEffect(item, comp, layer, effect);
        item.comp = comp.name;
        return comp;
    }

    function categoryItems(category) {
        var result = [];
        for (var i = 0; i < catalog.length; i++) {
            if (catalog[i].category === category) {
                result.push(catalog[i]);
            }
        }
        return result;
    }

    function importExistingPreview(item) {
        var previewFile = new File(previewRoot.fsName + "/" + item.slug + ".png");
        if (!previewFile.exists) {
            return null;
        }
        try {
            var footage = app.project.importFile(new ImportOptions(previewFile));
            footage.name = "CATASSET - " + item.name;
            log.push("IMPORTED existing preview asset: " + previewFile.fsName);
            return footage;
        } catch (error) {
            warnings.push("Could not import preview asset: " + previewFile.fsName + " (" + error.toString() + ")");
            return null;
        }
    }

    function makeGrid(category) {
        var items = category === "ALL" ? catalog : categoryItems(category);
        var columns = Math.min(4, items.length);
        var rows = Math.ceil(items.length / columns);
        var cell = 600;
        var comp = app.project.items.addComp("CATGRID - " + category, columns * cell, rows * cell, 1, 1, 30);
        comp.bgColor = [0.015, 0.02, 0.03];
        comp.layers.addSolid([0.015, 0.02, 0.03], "Background", comp.width, comp.height, 1, comp.duration);

        for (var i = 0; i < items.length; i++) {
            var col = i % columns;
            var row = Math.floor(i / columns);
            var x = col * cell + cell / 2;
            var y = row * cell + cell / 2;
            var card = comp.layers.addSolid([0.035, 0.045, 0.065], "Card - " + items[i].name, 560, 560, 1, comp.duration);
            card.property("ADBE Transform Group").property("ADBE Position").setValue([x, y]);

            var previewSource = findComp(items[i].comp);
            if (!previewSource) {
                previewSource = importExistingPreview(items[i]);
            }
            if (!previewSource) {
                warnings.push("Missing preview source: " + items[i].comp);
                continue;
            }
            var preview = comp.layers.add(previewSource);
            preview.name = "Preview - " + items[i].name;
            preview.property("ADBE Transform Group").property("ADBE Scale").setValue([82, 82]);
            preview.property("ADBE Transform Group").property("ADBE Position").setValue([x, y - 35]);

            var label = comp.layers.addText(items[i].name);
            label.name = "Label - " + items[i].name;
            var doc = label.property("ADBE Text Properties").property("ADBE Text Document").value;
            doc.fontSize = items[i].name.length > 22 ? 26 : 31;
            doc.fillColor = [0.94, 0.96, 1.0];
            doc.applyFill = true;
            doc.applyStroke = false;
            doc.justification = ParagraphJustification.CENTER_JUSTIFY;
            label.property("ADBE Text Properties").property("ADBE Text Document").setValue(doc);
            label.property("ADBE Transform Group").property("ADBE Position").setValue([x, y + 253]);
        }
        return comp;
    }

    function writeReport(extra) {
        ensureFolder(projectRoot);
        reportFile.encoding = "UTF-8";
        if (!reportFile.open("w")) {
            return;
        }
        var lines = [
            "After Effects: " + app.version,
            "Phase: " + phase,
            "Project: " + projectFile.fsName,
            "Catalog effects: " + catalog.length,
            "P: " + categoryItems("P").length,
            "A: " + categoryItems("A").length,
            "D: " + categoryItems("D").length,
            "G: " + categoryItems("G").length,
            ""
        ];
        lines = lines.concat(log);
        if (warnings.length) {
            lines.push("");
            lines.push("Warnings:");
            lines = lines.concat(warnings);
        }
        if (extra) {
            lines.push("");
            lines.push(extra);
        }
        reportFile.write(lines.join("\n"));
        reportFile.close();
    }

    ensureFolder(docsRoot);
    ensureFolder(previewRoot);
    ensureFolder(overviewRoot);
    ensureFolder(projectRoot);

    if (phase === "build" && seedProjectPath) {
        var seedProjectFile = new File(seedProjectPath);
        if (seedProjectFile.exists) {
            app.open(seedProjectFile);
        } else {
            warnings.push("Seed project not found: " + seedProjectPath);
        }
    } else if (phase !== "build" && projectFile.exists) {
        app.open(projectFile);
    }

    if (!app.project) {
        throw new Error("No After Effects project is open.");
    }

    app.beginUndoGroup("Build AOD plugin catalog");
    try {
        if (phase === "build") {
            removeCompsWithPrefix("CATPREVIEW - ");
            removeCompsWithPrefix("CATGRID - ");
            for (var i = 0; i < catalog.length; i++) {
                if (catalog[i].match) {
                    createOldPreview(catalog[i]);
                    log.push("BUILT " + catalog[i].aod + " -> " + catalog[i].comp);
                } else if (!findComp(catalog[i].comp)) {
                    var existingPreview = new File(previewRoot.fsName + "/" + catalog[i].slug + ".png");
                    if (existingPreview.exists) {
                        log.push("USING existing preview asset: " + existingPreview.fsName);
                    } else {
                        warnings.push("Existing preview comp and asset not found: " + catalog[i].comp);
                    }
                } else {
                    log.push("REUSED " + catalog[i].aod + " -> " + catalog[i].comp);
                }
            }
            makeGrid("P");
            makeGrid("A");
            makeGrid("D");
            makeGrid("G");
            makeGrid("ALL");
            app.project.save(projectFile);
            log.push("SAVED " + projectFile.fsName);
            writeReport("Build complete; run render-previews and render-overviews phases next.");
            notify("AOD catalog project built.\n" + projectFile.fsName);
        } else if (phase === "render-previews") {
            for (var p = 0; p < catalog.length; p++) {
                var previewFile = new File(previewRoot.fsName + "/" + catalog[p].slug + ".png");
                if (!catalog[p].fresh) {
                    log.push("KEPT existing preview: " + previewFile.fsName);
                    continue;
                }
                var previewComp = findComp(catalog[p].comp);
                if (!previewComp) {
                    if (previewFile.exists) {
                        log.push("KEPT existing preview: " + previewFile.fsName);
                    } else {
                        warnings.push("Missing preview comp and asset: " + catalog[p].comp);
                    }
                    continue;
                }
                previewComp.saveFrameToPng(Math.min(7 / 30, previewComp.duration / 2), previewFile);
                log.push("RENDERED " + previewFile.fsName);
            }
            app.project.save(projectFile);
            writeReport("Individual previews rendered.");
            notify("AOD catalog previews rendered.\n" + previewRoot.fsName);
        } else if (phase === "render-overviews") {
            var categories = ["P", "A", "D", "G", "ALL"];
            var names = { P: "photo", A: "anime", D: "map-data", G: "generator", ALL: "all-effects" };
            for (var c = 0; c < categories.length; c++) {
                var grid = findComp("CATGRID - " + categories[c]);
                if (!grid) {
                    warnings.push("Missing grid comp: CATGRID - " + categories[c]);
                    continue;
                }
                var gridFile = new File(overviewRoot.fsName + "/" + names[categories[c]] + ".png");
                grid.saveFrameToPng(Math.min(7 / 30, grid.duration / 2), gridFile);
                log.push("RENDERED " + gridFile.fsName);
            }
            app.project.save(projectFile);
            writeReport("Overview grids rendered.");
            notify("AOD catalog overview grids rendered.\n" + overviewRoot.fsName);
        } else {
            throw new Error("Unknown catalog phase: " + phase);
        }
    } finally {
        app.endUndoGroup();
        if (headless) {
            app.scheduleTask("app.quit()", 500, false);
        }
    }
}());
