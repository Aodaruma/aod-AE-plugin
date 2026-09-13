"""Copy project licensing and recorded notices into a release staging directory."""

# SPDX-License-Identifier: MPL-2.0

import argparse
import os
from pathlib import Path
import re
import shutil
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    parser.add_argument(
        "--repository",
        default=os.environ.get("GITHUB_REPOSITORY", "Aodaruma/aod-AE-plugin"),
        help="GitHub owner/repository containing the distributed source revision",
    )
    args = parser.parse_args()
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", args.repository):
        parser.error("repository must be a GitHub owner/repository")

    root = Path(__file__).resolve().parents[1]
    revision = subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=root, text=True
    ).strip()
    source = f"https://github.com/{args.repository}"
    tree = f"{source}/tree/{revision}"
    destination = args.destination.resolve()
    destination.mkdir(parents=True, exist_ok=True)

    files = [Path("LICENSE"), Path("templates/plugin/TEMPLATE-LICENSE.txt")]
    files.extend(sorted(root.glob("plugins/*/THIRD_PARTY_NOTICES*")))
    for item in files:
        relative = item.relative_to(root) if item.is_absolute() else item
        output = destination / relative
        output.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(root / relative, output)

    # Keep contribution links usable when this guide travels without the repo.
    guide = (root / "LICENSING.md").read_text(encoding="utf-8")
    guide = guide.replace("(CONTRIBUTING.md)", f"({tree}/CONTRIBUTING.md)")
    (destination / "LICENSING.md").write_text(guide, encoding="utf-8", newline="\n")
    (destination / "SOURCE.txt").write_text(
        "aod-AE-plugin source information\n\n"
        f"Repository: {source}\n"
        f"Revision: {revision}\n"
        f"Browse source: {tree}\n"
        f"Download source: {source}/archive/{revision}.zip\n\n"
        "The source for the distributed repository plugins and shared utilities\n"
        "is available at the revision above under MPL-2.0. See LICENSE.\n"
        "For dependency sources and versions, see Cargo.lock at that revision.\n"
        "Recorded plugin-specific notices are included under plugins/.\n"
        "The template permission applies only to the designated template material;\n"
        "it does not relicense existing effect implementations or dependencies.\n\n"
        "If you redistribute a modified build, provide its corresponding modified\n"
        "MPL source and update this source information. Upstream source alone does\n"
        "not describe your changes. Recipients retain their MPL source rights.\n",
        encoding="utf-8",
        newline="\n",
    )
    print(f"Packaged project licenses and {len(files) - 2} recorded third-party notice(s).")


if __name__ == "__main__":
    main()
