"""GUI entry point."""
from __future__ import annotations

import sys

from PySide6.QtWidgets import QApplication

from tectora.gui import style
from tectora.gui.main_window import MainWindow


def main() -> int:
    app = QApplication(sys.argv)
    app.setApplicationName("Tectora")
    app.setOrganizationName("Tectora")
    app.setStyleSheet(style.STYLESHEET)

    window = MainWindow()
    window.show()

    return app.exec()


if __name__ == "__main__":
    raise SystemExit(main())
