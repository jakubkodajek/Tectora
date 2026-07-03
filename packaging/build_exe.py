"""Build a single-file Tectora.exe with PyInstaller.

Usage:
    pip install -e .[gui,build]
    python packaging/build_exe.py

Output: dist/Tectora.exe (single file, windowed, with the app icon).
The icon is generated on the fly when packaging/tectora.ico is missing.
"""
from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# Qt plugin folders the app needs at runtime. PyInstaller's PySide6 hook is
# supposed to collect these, but with some Python/PySide6 combinations it
# silently collects none ("no Qt platform plugin could be initialized" on
# start) — so they are bundled explicitly. PySide6's __init__ registers
# <package>/plugins as a Qt library path, hence the mirrored destination.
_QT_PLUGIN_DIRS = ("platforms", "styles", "imageformats")


def _qt_plugin_args() -> list[str]:
    import PySide6
    plugins = Path(PySide6.__path__[0]) / "plugins"
    args: list[str] = []
    for name in _QT_PLUGIN_DIRS:
        src = plugins / name
        if src.is_dir():
            dest = Path("PySide6") / "plugins" / name
            args += ["--add-data", f"{src}{os.pathsep}{dest}"]
    return args


def main() -> int:
    icon = ROOT / "packaging" / "tectora.ico"
    if not icon.exists():
        subprocess.check_call(
            [sys.executable, str(ROOT / "packaging" / "make_icon.py"), str(icon)]
        )

    cmd = [
        sys.executable, "-m", "PyInstaller",
        "--noconfirm", "--clean",
        "--onefile", "--windowed",
        "--name", "Tectora",
        "--icon", str(icon),
        "--specpath", str(ROOT / "build"),
        "--workpath", str(ROOT / "build" / "pyinstaller"),
        "--distpath", str(ROOT / "dist"),
        *_qt_plugin_args(),
        str(ROOT / "packaging" / "launch_tectora.py"),
    ]
    return subprocess.call(cmd)


if __name__ == "__main__":
    raise SystemExit(main())
