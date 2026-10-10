#!/bin/bash
# SPDX-License-Identifier: MPL-2.0
# Build a self-contained universal development package on macOS.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
if [ "$(uname -s)" != Darwin ]; then
    echo 'This script requires macOS and the Xcode command line tools.' >&2
    exit 1
fi
for tool in cargo rustup clang nasm pkg-config python3 xcrun codesign; do
    command -v "$tool" >/dev/null || { echo "Missing build tool: $tool" >&2; exit 1; }
done
export MACOSX_DEPLOYMENT_TARGET=11.0
export CARGO_TARGET_DIR="$root/target"
work="$root/target/codec-map-macos"
sources="$work/sources"
mkdir -p "$sources" "$work/tmp"
export TMPDIR="$work/tmp"
jobs="$(sysctl -n hw.logicalcpu)"
sdk="$(xcrun --sdk macosx --show-sdk-path)"
ffmpeg_rev=38b88335f99e76ed89ff3c93f877fdefce736c13
x264_rev=b35605ace3ddf7c1a5d67a2eb553f034aef41d55
ffmpeg_sha=2ae7e42343cfffb811d15cfe98b6d005f082595fcdf034d30a4ff90cfed9f9c6
x264_sha=cd71a7515b0e9a012e1ac9b1f8415bebcaf6fc97d4db32286642ac4c0fbe24f9

fetch() {
    local name="$1" url="$2" sha="$3"
    if [ ! -f "$sources/$name" ]; then
        curl --fail --location --retry 3 "$url" -o "$sources/$name.partial"
        mv "$sources/$name.partial" "$sources/$name"
    fi
    echo "$sha  $sources/$name" | shasum -a 256 -c -
}
fetch ffmpeg-8.1.2.tar.gz "https://codeload.github.com/FFmpeg/FFmpeg/tar.gz/$ffmpeg_rev" "$ffmpeg_sha"
fetch x264.tar.gz "https://code.videolan.org/videolan/x264/-/archive/$x264_rev/x264-$x264_rev.tar.gz" "$x264_sha"

for arch in arm64 x86_64; do
    if [ "$arch" = arm64 ]; then rust_arch=aarch64; else rust_arch=x86_64; fi
    prefix="$work/$arch/install"
    mkdir -p "$work/$arch/x264" "$work/$arch/ffmpeg" "$prefix"
    tar -xzf "$sources/x264.tar.gz" -C "$work/$arch/x264" --strip-components=1
    tar -xzf "$sources/ffmpeg-8.1.2.tar.gz" -C "$work/$arch/ffmpeg" --strip-components=1
    (
        cd "$work/$arch/x264"
        CC="clang -arch $arch" ./configure --prefix="$prefix" \
            --host="$rust_arch-apple-darwin" --sysroot="$sdk" \
            --enable-static --enable-pic --disable-cli --disable-opencl \
            --bit-depth=8 --chroma-format=420 \
            --extra-cflags="-mmacosx-version-min=$MACOSX_DEPLOYMENT_TARGET" \
            --extra-ldflags="-mmacosx-version-min=$MACOSX_DEPLOYMENT_TARGET"
        make -j "$jobs"
        make install
    )
    (
        cd "$work/$arch/ffmpeg"
        PKG_CONFIG_PATH= PKG_CONFIG_LIBDIR="$prefix/lib/pkgconfig" ./configure \
            --prefix="$prefix" --arch="$rust_arch" --target-os=darwin \
            --enable-cross-compile --sysroot="$sdk" --cc="clang -arch $arch" \
            --extra-cflags="-mmacosx-version-min=$MACOSX_DEPLOYMENT_TARGET" \
            --extra-ldflags="-mmacosx-version-min=$MACOSX_DEPLOYMENT_TARGET -Wl,-headerpad_max_install_names" \
            --disable-autodetect --disable-everything --disable-programs --disable-doc \
            --disable-debug --disable-static --enable-shared --enable-pic \
            --disable-avdevice --disable-avformat --disable-avfilter \
            --disable-swresample --disable-swscale --disable-network \
            --enable-gpl --enable-libx264 --enable-encoder=libx264 \
            --enable-decoder=h264 --enable-parser=h264
        make -j "$jobs"
        make install
    )
    rustup target add "$rust_arch-apple-darwin"
    cargo build --locked -p codec_map --release --target "$rust_arch-apple-darwin"
done

version="$(python3 -c 'import tomllib; print(tomllib.load(open("plugins/codec-map/Cargo.toml", "rb"))["package"]["version"])')"
package="$work/AOD_CodecMap-$version-macos-universal"
bundle="$package/AOD_CodecMap.plugin"
# Recreate only this generated package, after checking its parent and basename.
python3 - "$work" "$package" <<'PY'
import pathlib, shutil, sys
parent, package = map(pathlib.Path, sys.argv[1:])
if package.is_symlink() or package.resolve().parent != parent.resolve() or not package.name.startswith('AOD_CodecMap-'):
    raise SystemExit('Unsafe package destination')
if package.exists():
    shutil.rmtree(package)
PY
runtime="$bundle/Contents/MacOS/CodecMap"
mkdir -p "$runtime" "$bundle/Contents/Resources" "$package/licenses" "$package/sources/build-info"
cp target/x86_64-apple-darwin/release/codec_map.rsrc "$bundle/Contents/Resources/AOD_CodecMap.rsrc"
cp target/x86_64-apple-darwin/release/codec_map_PkgInfo "$bundle/Contents/PkgInfo"
cp target/x86_64-apple-darwin/release/codec_map_Info.plist "$bundle/Contents/Info.plist"
/usr/libexec/PlistBuddy -c 'Set :CFBundleIdentifier com.aodaruma.AOD_CodecMap' "$bundle/Contents/Info.plist"
/usr/libexec/PlistBuddy -c 'Add :CFBundleExecutable string AOD_CodecMap' "$bundle/Contents/Info.plist"
lipo -create target/{aarch64,x86_64}-apple-darwin/release/libcodec_map.dylib \
    -output "$bundle/Contents/MacOS/AOD_CodecMap"
for lib in libavutil.60.dylib libavcodec.62.dylib; do
    lipo -create "$work/arm64/install/lib/$lib" "$work/x86_64/install/lib/$lib" -output "$runtime/$lib"
    install_name_tool -id "@loader_path/$lib" "$runtime/$lib"
done
# Rewrite every build-prefix dependency and reject unbundled dependencies.
python3 - "$bundle/Contents/MacOS" <<'PY'
import pathlib, subprocess, sys
directory = pathlib.Path(sys.argv[1])
for binary in [directory / 'AOD_CodecMap', *sorted((directory / 'CodecMap').glob('*.dylib'))]:
    output = subprocess.check_output(['otool', '-L', str(binary)], text=True)
    dependencies = {line.strip().split(' (compatibility version')[0] for line in output.splitlines() if line.startswith('\t')}
    for dep in sorted(dependencies):
        if dep.startswith(('/usr/lib/', '/System/Library/', '@loader_path/')):
            continue
        if (directory / 'CodecMap' / pathlib.Path(dep).name).is_file():
            subprocess.run(['install_name_tool', '-change', dep, '@loader_path/' + pathlib.Path(dep).name, str(binary)], check=True)
        elif binary.name == 'AOD_CodecMap' and pathlib.Path(dep).name == 'libcodec_map.dylib':
            subprocess.run(['install_name_tool', '-id', '@loader_path/AOD_CodecMap', str(binary)], check=True)
        else:
            raise SystemExit(f'Unbundled dependency: {binary.name}: {dep}')
    subprocess.run(['lipo', '-verify_arch', 'arm64', 'x86_64', str(binary)], check=True)
PY
for lib in "$runtime"/*.dylib; do
    codesign --force --sign - --timestamp=none "$lib"
done
codesign --force --sign - --timestamp=none "$bundle"
codesign --verify --deep --strict --verbose=2 "$bundle"
plutil -lint "$bundle/Contents/Info.plist"

# Test the exact bundled runtime on the runner's native architecture.
export CODECMAP_CODEC_DIR="$runtime"
cargo test --locked -p codec_map -- --include-ignored
cp LICENSE "$package/licenses/MPL-2.0.txt"
cp plugins/codec-map/TEMPLATE-LICENSE.txt "$package/licenses/"
cp "$work/arm64/ffmpeg/COPYING.GPLv2" "$package/licenses/FFmpeg-GPL-2.0.txt"
cp "$work/arm64/ffmpeg/COPYING.LGPLv2.1" "$package/licenses/FFmpeg-LGPL-2.1.txt"
cp "$work/arm64/x264/COPYING" "$package/licenses/x264-COPYING.txt"
cp "$sources/ffmpeg-8.1.2.tar.gz" "$sources/x264.tar.gz" "$package/sources/"
for arch in arm64 x86_64; do
    mkdir -p "$package/sources/build-info/$arch"
    cp "$work/$arch/ffmpeg/ffbuild/config.log" "$package/sources/build-info/$arch/ffmpeg-config.log"
    cp "$work/$arch/x264/config.log" "$package/sources/build-info/$arch/x264-config.log"
done
tar -czf "$package/sources/codec-map-source.tar.gz" \
    Cargo.toml Cargo.lock LICENSE LICENSING.md AdobePlugin.just \
    plugins/codec-map crates/utils scripts/prepare-codec-map-macos.sh
cp plugins/codec-map/README.md "$package/PLUGIN-README.md"
cat > "$package/README.txt" <<EOF
AOD_CodecMap $version / macOS Universal（Apple Silicon・Intel）

After Effectsを終了し、AOD_CodecMap.pluginを次のフォルダーへコピーしてください。
/Library/Application Support/Adobe/Common/Plug-ins/7.0/MediaCore/
再起動後、Aodaruma > AOD_CodecMapから使用できます。

FFmpeg 8.1.2 / libx264はプラグイン内に同梱しています。別途インストールは不要です。
開発検証用のアドホック署名です。Developer ID署名・Apple公証はありません。
このZIPを作成したCIではmacOS版AEの起動・描画テストは行っていません。
コーデックの実行テストはビルドしたMacのネイティブCPUで実行しています。
対応ソース、ビルド設定、ライセンスはsources/、licenses/に含まれます。
EOF
{
    echo "Source revision: $(git rev-parse HEAD)"
    echo "FFmpeg revision: $ffmpeg_rev"
    echo "x264 revision: $x264_rev"
    echo "MACOSX_DEPLOYMENT_TARGET=$MACOSX_DEPLOYMENT_TARGET"
    rustc --version
    xcrun clang --version
    sw_vers
    shasum -a 256 "$sources/ffmpeg-8.1.2.tar.gz" "$sources/x264.tar.gz"
} > "$package/sources/BUILD.txt"
zip="$work/$(basename "$package").zip"
ditto -c -k --sequesterRsrc --keepParent "$package" "$zip"
shasum -a 256 "$zip" > "$zip.sha256"
echo "Package: $zip"
