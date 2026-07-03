"""GUI entry point."""
from __future__ import annotations

import sys

from PySide6.QtWidgets import QApplication

from tectora.gui import theme
from tectora.gui.app_window import MainWindow


def main() -> int:
    argv = list(sys.argv)
    # --smoke: start the app, then quit as soon as the event loop is up.
    # Lets CI and the installer build verify that the packaged exe launches.
    smoke = "--smoke" in argv
    if smoke:
        argv.remove("--smoke")
    app = QApplication(argv)
    theme.apply_theme(app)
    window = MainWindow()
    window.show()
    if smoke:
        from PySide6.QtCore import QTimer
        QTimer.singleShot(0, app.quit)
    return app.exec()


if __name__ == "__main__":
    sys.exit(main())
