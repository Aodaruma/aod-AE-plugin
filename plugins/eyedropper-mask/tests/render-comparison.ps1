$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$taskRoot = (Resolve-Path (Join-Path $PSScriptRoot '../../..')).Path
$taskOutput = Join-Path $taskRoot 'target/ae-gap-test'
$taskCanvas = [Drawing.Bitmap]::new(840, 235)
$taskGraphics = [Drawing.Graphics]::FromImage($taskCanvas)
$taskFont = [Drawing.Font]::new('Yu Gothic UI', 14)
$taskSmallFont = [Drawing.Font]::new('Yu Gothic UI', 11)
$taskDark = [Drawing.SolidBrush]::new([Drawing.Color]::FromArgb(64, 70, 79))
$taskLight = [Drawing.SolidBrush]::new([Drawing.Color]::FromArgb(89, 96, 106))
try {
    $taskGraphics.Clear([Drawing.Color]::FromArgb(24, 29, 37))
    $taskGraphics.DrawString('縮小で境界に小さな穴が開くケース：左側だけを透明化', $taskFont, [Drawing.Brushes]::White, 20, 12)
    $taskCases = @(
        @{ Label='Full'; File='narrow-bpc8-source2-stable1-res1.png' },
        @{ Label='Quarter / 保護 OFF'; File='narrow-bpc8-source2-stable0-res4.png' },
        @{ Label='Quarter / 保護 ON (1 px)'; File='narrow-bpc8-source2-stable1-res4.png' }
    )
    for ($taskIndex = 0; $taskIndex -lt $taskCases.Count; $taskIndex++) {
        $taskLeft = 20 + $taskIndex * 280
        $taskGraphics.DrawString($taskCases[$taskIndex].Label, $taskFont, [Drawing.Brushes]::White, $taskLeft, 46)
        for ($taskY = 0; $taskY -lt 120; $taskY += 10) {
            for ($taskX = 0; $taskX -lt 240; $taskX += 10) {
                $taskBrush = if ((($taskX + $taskY) / 10) % 2 -eq 0) { $taskDark } else { $taskLight }
                $taskGraphics.FillRectangle($taskBrush, $taskLeft + $taskX, 78 + $taskY, 10, 10)
            }
        }
        $taskImage = [Drawing.Bitmap]::FromFile((Join-Path $taskOutput $taskCases[$taskIndex].File))
        try {
            $taskGraphics.InterpolationMode = [Drawing.Drawing2D.InterpolationMode]::NearestNeighbor
            $taskGraphics.PixelOffsetMode = [Drawing.Drawing2D.PixelOffsetMode]::Half
            $taskGraphics.DrawImage($taskImage, [Drawing.Rectangle]::new($taskLeft, 78, 240, 120))
        } finally { $taskImage.Dispose() }
    }
    $taskGraphics.DrawString('AE 26.3 実出力 / 8 bpc / Input Layer Data / Color Threshold 20% / チェッカーは透明部分', $taskSmallFont, [Drawing.Brushes]::LightGray, 20, 209)
    $taskPath = Join-Path $taskRoot 'docs/catalog/assets/previews/eyedropper-mask-resolution.png'
    $taskCanvas.Save($taskPath, [Drawing.Imaging.ImageFormat]::Png)
    Write-Output $taskPath
} finally {
    $taskGraphics.Dispose(); $taskCanvas.Dispose(); $taskFont.Dispose(); $taskSmallFont.Dispose(); $taskDark.Dispose(); $taskLight.Dispose()
}
