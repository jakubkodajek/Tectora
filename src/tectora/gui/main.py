"""GUI entry point."""
from __future__ import annotations

import sys

from PySide6.QtWidgets import QApplication

from tectora.gui import theme
from tectora.gui.app_window import MainWindow


def main() -> int:
    app = QApplication(sys.argv)
    theme.apply_theme(app)
    window = MainWindow()
    window.show()
    return app.exec()


if __name__ == "__main__":
    sys.exit(main())
