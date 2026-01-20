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
