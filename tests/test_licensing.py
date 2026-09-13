"""Check license boundaries in real generated projects and release staging."""

# SPDX-License-Identifier: MPL-2.0

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import tomllib
import unittest
import zipfile


ROOT = Path(__file__).resolve().parents[1]


def run(*args, cwd=ROOT, env=None):
    result = subprocess.run(
        args, cwd=cwd, env=env, text=True, encoding="utf-8", errors="replace",
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
    )
    if result.returncode:
        raise AssertionError(f"Command failed: {' '.join(map(str, args))}\n{result.stdout}")
    return result.stdout


class LicensingTests(unittest.TestCase):
    def test_existing_packages_and_mpl_text(self):
        packages = json.loads(run("cargo", "metadata", "--no-deps", "--format-version", "1"))["packages"]
        self.assertTrue(packages)
        for package in packages:
            self.assertEqual(package["license"], "MPL-2.0", package["name"])
        permission = (ROOT / "templates/plugin/TEMPLATE-LICENSE.txt").read_text(encoding="utf-8")
        self.assertTrue(permission.endswith((ROOT / "LICENSE").read_text(encoding="utf-8")))

    def test_generation_and_compilation(self):
        target = ROOT / "target"
        target.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="licensing-", dir=target) as directory:
            workspace = Path(directory).resolve()
            self.assertTrue(workspace.is_relative_to(target.resolve()))
            for name in ("Cargo.toml", "Cargo.lock"):
                shutil.copyfile(ROOT / name, workspace / name)
            # Cargo inherits the real alias from ROOT/.cargo. Copying that array
            # into this nested workspace would concatenate the alias twice.
            for name in ("crates/utils", "templates/plugin"):
                shutil.copytree(ROOT / name, workspace / name)
            (workspace / "plugins").mkdir()
            for repository in (False, True):
                for gpu in (False, True):
                    name = f"{'repo' if repository else 'external'}-{'gpu' if gpu else 'cpu'}"
                    with self.subTest(name=name):
                        values = workspace / "values.toml"
                        values.write_text(
                            '[values]\ndescription = "Licensing smoke test."\n'
                            f'features = {json.dumps(["wgpu"] if gpu else [])}\n'
                            'with_deepcolor = true\nwith_thrededrender = true\n'
                            'with_smartrender = true\n'
                            'plugin_author = "Example Studio"\n'
                            'plugin_category = "Studio Effects"\n'
                            'plugin_support_url = "https://example.org/support"\n',
                            encoding="utf-8",
                        )
                        command = ["cargo", "new-plugin"] if repository else [
                            "cargo", "generate", "--path", "templates/plugin",
                            "--destination", "plugins",
                        ]
                        # Internal generation exercises the alias; external uses the
                        # default mode, so a future default change cannot go unnoticed.
                        run(*command, "--name", name, "--silent", "--values-file", str(values),
                            "--no-workspace", "--vcs", "none", cwd=workspace)
                        plugin = workspace / "plugins" / name
                        manifest = tomllib.loads((plugin / "Cargo.toml").read_text(encoding="utf-8"))
                        package = manifest["package"]
                        self.assertEqual(
                            (plugin / "TEMPLATE-LICENSE.txt").read_bytes(),
                            (ROOT / "templates/plugin/TEMPLATE-LICENSE.txt").read_bytes(),
                        )
                        self.assertEqual((plugin / "src/gpu").exists(), gpu)
                        self.assertFalse((plugin / "pre-script.rhai").exists())
                        build = (plugin / "build.rs").read_text(encoding="utf-8")
                        if repository:
                            self.assertEqual(package["license"], {"workspace": True})
                            self.assertNotIn("license-file", package)
                            self.assertIn('Property::Category("Aodaruma")', build)
                        else:
                            self.assertNotIn("license", package)
                            self.assertEqual(package["license-file"], "TEMPLATE-LICENSE.txt")
                            self.assertEqual(package["authors"], ["Example Studio"])
                            self.assertEqual(package["publish"], False)
                            self.assertIn('Property::Category("Studio Effects")', build)
                            self.assertIn("https://example.org/support", build)
                        for path in plugin.rglob("*"):
                            if path.is_file() and path.suffix in (".rs", ".wgsl"):
                                text = path.read_text(encoding="utf-8")
                                self.assertIn("TEMPLATE-LICENSE.txt", text, path)
                                self.assertNotIn("{{", text, path)
            print("Checking four generated plugin variants...", flush=True)
            env = os.environ.copy()
            env["CARGO_TARGET_DIR"] = str(target.resolve())
            run("cargo", "check", "--workspace", cwd=workspace, env=env)

    def test_release_notices_survive_archiving(self):
        target = ROOT / "target"
        target.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="license-archive-", dir=target) as directory:
            temporary = Path(directory).resolve()
            self.assertTrue(temporary.is_relative_to(target.resolve()))
            stage = temporary / "dist"
            run(sys.executable, str(ROOT / "scripts/package_licenses.py"), str(stage),
                "--repository", "example/fork")
            revision = run("git", "rev-parse", "HEAD").strip()
            source = (stage / "SOURCE.txt").read_text(encoding="utf-8")
            self.assertIn(f"https://github.com/example/fork/archive/{revision}.zip", source)
            self.assertEqual((stage / "LICENSE").read_bytes(), (ROOT / "LICENSE").read_bytes())
            archive = shutil.make_archive(str(temporary / "release"), "zip", stage)
            with zipfile.ZipFile(archive) as zipped:
                self.assertIn("SOURCE.txt", zipped.namelist())
                self.assertIn("templates/plugin/TEMPLATE-LICENSE.txt", zipped.namelist())
                notices = list(ROOT.glob("plugins/*/THIRD_PARTY_NOTICES*"))
                self.assertTrue(notices)
                for notice in notices:
                    relative = notice.relative_to(ROOT).as_posix()
                    self.assertEqual(zipped.read(relative), notice.read_bytes())


if __name__ == "__main__":
    unittest.main()
