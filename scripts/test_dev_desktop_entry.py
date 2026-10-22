"""Headless regression checks; never change the real user's desktop integration."""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parent.parent


class DesktopEntryTest(unittest.TestCase):
    def test_install_with_spaces_and_no_flatpak(self):
        with tempfile.TemporaryDirectory(prefix="rspotify desktop ") as directory:
            root = Path(directory)
            data = root / "data"
            binary = root / "binary with spaces"
            shutil.copy2("/bin/true", binary)
            stubs = root / "tools"
            stubs.mkdir()
            for tool in ("gtk-update-icon-cache", "kbuildsycoca6", "kbuildsycoca5"):
                path = stubs / tool
                path.write_text("#!/bin/sh\nexit 0\n")
                path.chmod(0o755)
            env = dict(os.environ, XDG_DATA_HOME=str(data), XDG_CURRENT_DESKTOP="",
                       PATH=str(stubs) + os.pathsep + os.environ["PATH"])
            command = ["sh", str(ROOT / "scripts/install-dev-desktop-entry.sh"), str(binary)]
            subprocess.run(command, env=env, check=True, capture_output=True)
            desktop = data / "applications/rspotify.desktop"
            icon = data / "icons/hicolor/scalable/apps/rspotify.svg"
            self.assertEqual(icon.read_bytes(), (ROOT / "assets/icons/rspotify.svg").read_bytes())
            self.assertIn(f'Exec="{binary.resolve()}"', desktop.read_text())
            self.assertIn(f"Icon={icon.resolve()}", desktop.read_text())
            self.assertNotIn("com.spotify.Client", desktop.read_text())
            self.assertIn("StartupWMClass=rspotify", desktop.read_text())
            subprocess.run(["desktop-file-validate", str(desktop)], check=True)
            first = desktop.read_bytes()
            subprocess.run(command, env=env, check=True, capture_output=True)
            self.assertEqual(first, desktop.read_bytes())

    def test_missing_executable_does_not_install(self):
        with tempfile.TemporaryDirectory() as directory:
            data = Path(directory) / "data"
            result = subprocess.run(
                ["sh", str(ROOT / "scripts/install-dev-desktop-entry.sh"), str(data / "missing")],
                env=dict(os.environ, XDG_DATA_HOME=str(data)), capture_output=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(data.exists())


if __name__ == "__main__":
    unittest.main()
