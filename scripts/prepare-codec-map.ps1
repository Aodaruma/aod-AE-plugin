# SPDX-License-Identifier: MPL-2.0
[CmdletBinding()]
param(
    [ValidateSet('debug', 'release')][string]$Profile = 'release',
    [switch]$SkipBuild,
    [string]$InstallDirectory
)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$target = Join-Path $root 'target'
$setup = Join-Path $target 'codec-map-setup'
$version = '8.1.2'
$archive = Join-Path $setup "ffmpeg-$version.zip"
$expected = '274923c68904a9b76c73b908f57923dafba81155856cd742138515ded570d066'
$url = "https://github.com/GyanD/codexffmpeg/releases/download/$version/ffmpeg-$version-full_build-shared.zip"
New-Item -ItemType Directory -Force -Path $setup | Out-Null
if (-not (Test-Path -LiteralPath $archive)) {
    Invoke-WebRequest -Uri $url -OutFile $archive
}
if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expected) {
    throw "FFmpeg archive checksum mismatch: $archive"
}
$expanded = Join-Path $setup 'ffmpeg'
$runtime = Join-Path $expanded "ffmpeg-$version-full_build-shared"
if (-not (Test-Path -LiteralPath (Join-Path $runtime 'bin/avcodec-62.dll'))) {
    Expand-Archive -LiteralPath $archive -DestinationPath $expanded -Force
}
if (-not $SkipBuild) {
    Push-Location $root
    try {
        if ($Profile -eq 'release') { cargo build -p codec_map --release } else { cargo build -p codec_map }
        if ($LASTEXITCODE -ne 0) { throw 'CodecMap build failed' }
    } finally { Pop-Location }
}
$binary = Join-Path $target "$Profile/codec_map.dll"
if (-not (Test-Path -LiteralPath $binary)) { throw "Plugin not built: $binary" }
$package = Join-Path $target "$Profile/AOD_CodecMap-package"
$libraries = Join-Path $package 'CodecMap'
New-Item -ItemType Directory -Force -Path $libraries | Out-Null
Copy-Item -LiteralPath $binary -Destination (Join-Path $package 'AOD_CodecMap.aex') -Force
foreach ($dll in @('avcodec-62.dll', 'avutil-60.dll', 'swresample-6.dll')) {
    Copy-Item -LiteralPath (Join-Path $runtime "bin/$dll") -Destination $libraries -Force
}
Copy-Item -LiteralPath (Join-Path $runtime 'LICENSE') -Destination (Join-Path $libraries 'FFmpeg-LICENSE.txt') -Force
Copy-Item -LiteralPath (Join-Path $runtime 'README.txt') -Destination (Join-Path $libraries 'FFmpeg-README.txt') -Force
Copy-Item -LiteralPath (Join-Path $root 'plugins/codec-map/native/README.md') -Destination (Join-Path $libraries 'SOURCES.md') -Force
Copy-Item -LiteralPath (Join-Path $root 'plugins/codec-map/native/COPYING.LGPLv2.1') -Destination $libraries -Force
Copy-Item -LiteralPath (Join-Path $root 'LICENSE') -Destination (Join-Path $package 'MPL-2.0.txt') -Force
Copy-Item -LiteralPath (Join-Path $root 'plugins/codec-map/README.md') -Destination $package -Force
if ($InstallDirectory) {
    $destination = [IO.Path]::GetFullPath($InstallDirectory)
    New-Item -ItemType Directory -Force -Path $destination | Out-Null
    Copy-Item -LiteralPath (Join-Path $package 'AOD_CodecMap.aex') -Destination $destination -Force
    Copy-Item -LiteralPath $libraries -Destination $destination -Recurse -Force
    Write-Output "Installed: $destination"
}
Write-Output "Local development package: $package"
Write-Output 'FFmpeg/libx264 runtime is GPL. See CodecMap/SOURCES.md before redistribution.'
