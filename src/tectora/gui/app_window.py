"""Main window: stacked Home <-> Editor navigation."""
from __future__ import annotations

from PySide6.QtWidgets import (QMainWindow, QStackedWidget)

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

        # G1: real HomeView
        from tectora.gui.home.home_view import HomeView
        self._home = HomeView(self)
        self._stack.addWidget(self._home)      # index 0

        # G2: real EditorView (imported lazily to avoid circular imports at
        # module level — EditorView receives the window instance at runtime)
        from tectora.gui.editor.editor_view import EditorView
        self._editor = EditorView(self)
        self._stack.addWidget(self._editor)    # index 1

        self.open_home()

    def open_home(self) -> None:
        self.refresh_home()
        self._stack.setCurrentIndex(0)

    def open_editor(self, project=None, entry_id=None) -> None:
        self._current_project = project
        self._current_entry_id = entry_id
        self._editor.load_project(project, entry_id)
        self._editor._current_entry_id = entry_id  # G6: keep editor in sync
        self._stack.setCurrentIndex(1)

    def open_entry(self, entry) -> None:
        """Open an existing library entry in the editor (G6)."""
        self._editor.load_project(
            entry.project, entry.id, note=entry.note, location=entry.location
        )
        self._editor._current_entry_id = entry.id
        self._stack.setCurrentIndex(1)

    def refresh_home(self) -> None:
        """Refresh the home view grid (e.g. after a delete)."""
        home = self._stack.widget(0)
        if hasattr(home, "refresh"):
            home.refresh()
