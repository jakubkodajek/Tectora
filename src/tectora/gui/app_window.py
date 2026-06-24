"""Main window: stacked Home <-> Editor navigation."""
from __future__ import annotations

from PySide6.QtWidgets import (QLabel, QMainWindow, QStackedWidget, QVBoxLayout, QWidget)

from tectora.io.sqlite_library import SqliteProjectLibrary, default_db_path


class MainWindow(QMainWindow):
    def __init__(self, library=None, lang: str = "cs"):
        super().__init__()
        self.library = library or SqliteProjectLibrary(default_db_path())
        self.lang = lang
        self.setWindowTitle("Tectora")
        self.resize(1100, 720)

        self._stack = QStackedWidget()
        self.setCentralWidget(self._stack)

        # G1: real HomeView; G2+ will replace editor placeholder.
        from tectora.gui.home.home_view import HomeView
        self._home = HomeView(self)
        self._editor = self._placeholder("Editor")
        self._stack.addWidget(self._home)      # index 0
        self._stack.addWidget(self._editor)    # index 1

        self.open_home()

    @staticmethod
    def _placeholder(text: str) -> QWidget:
        w = QWidget()
        layout = QVBoxLayout(w)
        layout.addWidget(QLabel(text))
        return w

    def open_home(self) -> None:
        self._stack.setCurrentIndex(0)

    def open_editor(self, project=None, entry_id=None) -> None:
        self._current_project = project
        self._current_entry_id = entry_id
        self._stack.setCurrentIndex(1)

    def refresh_home(self) -> None:
        """Refresh the home view grid (e.g. after a delete)."""
        home = self._stack.widget(0)
        if hasattr(home, "refresh"):
            home.refresh()
