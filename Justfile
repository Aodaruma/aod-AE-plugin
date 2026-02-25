set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]

[windows]
build:
	$ErrorActionPreference = "Stop"; $root = "{{justfile_directory()}}";	$justfiles = Get-ChildItem -Path (Join-Path $root "plugins") -Filter Justfile -Recurse;	$justfiles | ForEach-Object { just -f $_.FullName build }

[windows]
release:
	$ErrorActionPreference = "Stop"; $root = "{{justfile_directory()}}"; $justfiles = Get-ChildItem -Path (Join-Path $root "plugins") -Filter Justfile -Recurse; $justfiles | ForEach-Object { just -f $_.FullName release }

[macos]
build:
	#!/bin/bash
	set -euo pipefail
	root="{{justfile_directory()}}"
	find "$root/plugins" -name Justfile -type f -print0 | xargs -0 -I {} just -f "{}" build

[macos]
release:
	#!/bin/bash
	set -euo pipefail
	root="{{justfile_directory()}}"
	find "$root/plugins" -name Justfile -type f -print0 | xargs -0 -I {} just -f "{}" release

[windows]
release-ci:
	$ErrorActionPreference = "Stop"
	cargo build --workspace --release
	$root = "{{justfile_directory()}}"
	$justfiles = Get-ChildItem -Path (Join-Path $root "plugins") -Filter Justfile -Recurse
	$env:NO_CARGO_BUILD = "1"
	try { \
		$justfiles | ForEach-Object { just -f $_.FullName release } \
	} finally { \
		Remove-Item Env:NO_CARGO_BUILD -ErrorAction SilentlyContinue \
	}

[macos]
release-ci:
	#!/bin/bash
	set -euo pipefail
	root="{{justfile_directory()}}"
	rustup target add x86_64-apple-darwin
	rustup target add aarch64-apple-darwin
	cargo build --workspace --release --target x86_64-apple-darwin
	cargo build --workspace --release --target aarch64-apple-darwin
	find "$root/plugins" -name Justfile -type f -print0 | while IFS= read -r -d '' justfile; do \
		NO_CARGO_BUILD=1 just -f "$justfile" release; \
	done
