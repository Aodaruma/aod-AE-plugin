set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]

[windows]
build:
	$ErrorActionPreference = "Stop"
	$root = "{{justfile_directory()}}"
	$justfiles = Get-ChildItem -Path (Join-Path $root "plugins") -Filter Justfile -Recurse -File
	$justfiles | ForEach-Object {
		# 子 Justfile の場所を working directory に固定して実行
		just --justfile $_.FullName --working-directory $_.DirectoryName build
	}

[windows]
release:
	$ErrorActionPreference = "Stop"
	$root = "{{justfile_directory()}}"
	$justfiles = Get-ChildItem -Path (Join-Path $root "plugins") -Filter Justfile -Recurse -File
	$justfiles | ForEach-Object {
		just --justfile $_.FullName --working-directory $_.DirectoryName release
	}

[macos]
build:
	#!/bin/bash
	set -euo pipefail
	root="{{justfile_directory()}}"
	while IFS= read -r -d '' jf; do
		dir="$(dirname "$jf")"
		just --justfile "$jf" --working-directory "$dir" build
	done < <(find "$root/plugins" -name Justfile -type f -print0)

[macos]
release:
	#!/bin/bash
	set -euo pipefail
	root="{{justfile_directory()}}"
	while IFS= read -r -d '' jf; do
		dir="$(dirname "$jf")"
		just --justfile "$jf" --working-directory "$dir" release
	done < <(find "$root/plugins" -name Justfile -type f -print0)
