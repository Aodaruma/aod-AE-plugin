# SPDX-License-Identifier: MPL-2.0
[CmdletBinding()]
param([switch]$ValidateOnly)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '../../..')).Path
$output = Join-Path $root 'target/codec-map-smoke'
New-Item -ItemType Directory -Force -Path $output | Out-Null
Add-Type -AssemblyName System.Drawing
if (-not $ValidateOnly) {
    $source = [Drawing.Bitmap]::new(129, 97)
    $map = [Drawing.Bitmap]::new(129, 97)
    try {
        for ($y = 0; $y -lt 97; $y++) {
            for ($x = 0; $x -lt 129; $x++) {
                $v = ($x * 17 + $y * 31) % 251
                $a = if ($y -gt 80) { 128 } else { 255 }
                $source.SetPixel($x, $y, [Drawing.Color]::FromArgb($a, $v, ($v * 3) % 255, ($v * 7) % 255))
                $m = if ($x -lt 64) { 0 } else { 255 }
                $map.SetPixel($x, $y, [Drawing.Color]::FromArgb(255, $m, $m, $m))
            }
        }
        $source.Save((Join-Path $output 'source.png'))
        $map.Save((Join-Path $output 'map.png'))
    } finally { $source.Dispose(); $map.Dispose() }
    Write-Output "Fixtures prepared: $output"
    Write-Output 'Run smoke.jsx in an empty AE project, then run this script with -ValidateOnly.'
    exit 0
}
$report = [Collections.Generic.List[string]]::new()
function Compare-Images([string]$Reference, [string]$Result, [string]$Rule) {
    foreach ($name in @($Reference, $Result)) {
        $imagePath = Join-Path $output "$name.png"
        if (-not (Test-Path -LiteralPath $imagePath) -or (Get-Item -LiteralPath $imagePath).Length -eq 0) {
            throw "AE render did not produce $imagePath; inspect the AE error dialog and host-report.txt"
        }
    }
    $a = [Drawing.Bitmap]::new((Join-Path $output "$Reference.png"))
    $b = [Drawing.Bitmap]::new((Join-Path $output "$Result.png"))
    try {
        if ($a.Width -ne $b.Width -or $a.Height -ne $b.Height) { throw "Size mismatch: $Result" }
        $different = 0
        for ($y = 0; $y -lt $a.Height; $y++) {
            for ($x = 0; $x -lt $a.Width; $x++) {
                $p = $a.GetPixel($x, $y); $q = $b.GetPixel($x, $y)
                if ($p.A -ne $q.A) { throw "Alpha changed: $Result at $x,$y" }
                if ($p.ToArgb() -ne $q.ToArgb()) {
                    $different++
                    if ($Rule -eq 'equal' -or ($Rule -eq 'left-exact' -and $x -lt 64)) {
                        throw "Pixel changed unexpectedly: $Result at $x,$y"
                    }
                }
            }
        }
        if ($Rule -ne 'equal' -and $different -eq 0) { throw "Expected compression difference: $Result" }
        $report.Add("PASS $Reference -> $Result ($Rule): $different changed pixels")
    } finally { $a.Dispose(); $b.Dispose() }
}
Compare-Images 'reference' 'isolated8' 'left-exact'
Compare-Images 'reference' 'zero-mix8' 'equal'
Compare-Images 'reference' 'missing-map8' 'equal'
Compare-Images 'isolated8' 'replay8' 'equal'
Compare-Images 'reference' 'full8' 'different'
Compare-Images 'full8' 'independent8' 'different'
Compare-Images 'isolated8' 'past-map-edit8' 'different'
foreach ($bits in @(16, 32)) {
    Compare-Images "reference$bits" "isolated$bits" 'left-exact'
    Compare-Images "reference$bits" "zero-mix$bits" 'equal'
}
$report | Set-Content -LiteralPath (Join-Path $output 'pixel-report.txt') -Encoding UTF8
$report
