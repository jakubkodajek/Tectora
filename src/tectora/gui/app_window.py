"""Main window: stacked Home <-> Editor navigation."""
from __future__ import annotations

from PySide6.QtWidgets import (QMainWindow, QStackedWidget)

from tectora.io.sqlite_library import SqliteProjectLibrary, default_db_path


class MainWindow(QMainWindow):
    def __init__(self, library=None, lang: str = "cs"):
        super().__init__()
        self.library = library or SqliteProjectLibrary(default_db_path())
        self.lang = lang
        self.dark = False
        self.mode = "light"
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
        # Track current project/entry on the window so a theme/language change
        # (which tears down and rebuilds both views via set_language) restores
        # this project instead of an empty editor.
        self._current_project = entry.project
        self._current_entry_id = entry.id
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

    # ------------------------------------------------------------------
    # G8: Language switcher
    # ------------------------------------------------------------------

    def set_language(self, lang: str) -> None:
        """Switch UI language and rebuild both views (G8)."""
        if lang not in ("cs", "sk", "en"):
            return
        self.lang = lang

        # Capture current state before teardown
        current_idx = self._stack.currentIndex()

        # Capture editor state
        editor = self._editor
        saved_project = getattr(self, "_current_project", None)
        saved_entry_id = getattr(self, "_current_entry_id", None)
        saved_name = editor.get_name() if hasattr(editor, "get_name") else ""
        saved_note = editor.get_note() if hasattr(editor, "get_note") else ""
        saved_location = editor.get_location() if hasattr(editor, "get_location") else None
        saved_pitch = getattr(editor, "pitch", 35)
        # Capture ALL buildings (geometry + per-building params), not just the
        # active one, so a multi-building design survives a language/theme change.
        saved_buildings = (
            editor._collect_buildings() if hasattr(editor, "_collect_buildings") else []
        )

        # Rebuild HomeView (index 0)
        from tectora.gui.home.home_view import HomeView
        old_home = self._stack.widget(0)
        new_home = HomeView(self)
        self._stack.insertWidget(0, new_home)
        self._stack.removeWidget(old_home)
        old_home.deleteLater()
        self._home = new_home

        # Rebuild EditorView (index 1)
        from tectora.gui.editor.editor_view import EditorView
        old_editor = self._stack.widget(1)
        new_editor = EditorView(self)
        self._stack.insertWidget(1, new_editor)
        self._stack.removeWidget(old_editor)
        old_editor.deleteLater()
        self._editor = new_editor

        # Restore editor state if there was a project or any drawn building.
        if saved_project is not None or saved_buildings:
            from tectora.application.project import Project
            # Reconstruct the project with ALL buildings; load_project restores
            # every building (geometry + per-building pitch/overhang/wall) and
            # recomputes — so multi-building designs survive the rebuild.
            proj = Project(
                project_name=saved_name or getattr(saved_project, "project_name", ""),
                buildings=saved_buildings or None,
            )
            new_editor.load_project(
                proj, saved_entry_id,
                note=saved_note,
                location=saved_location,
            )
            new_editor._current_entry_id = saved_entry_id
            self._current_project = proj
            self._current_entry_id = saved_entry_id

        # Restore which view was showing
        self._stack.setCurrentIndex(current_idx)

    # ------------------------------------------------------------------
    # F2: Dark theme switcher
    # ------------------------------------------------------------------

    def set_theme(self, dark: bool) -> None:
        """Switch between light and dark theme and rebuild both views."""
        from tectora.gui import theme as _theme
        from PySide6.QtWidgets import QApplication

        _theme.set_dark(dark)
        app = QApplication.instance()
        if app is not None:
            _theme.apply_theme(app)
        self.dark = dark

        # Rebuild views so freshly constructed widgets read the new palette.
        # Reuse the same logic as set_language (captures + restores state).
        current_lang = self.lang
        self.set_language(current_lang)

    def _resolve_mode_dark(self, mode: str) -> bool:
        """Resolve an appearance *mode* to a dark boolean.

        "dark" -> True, "light" -> False, "system" -> query the OS colour
        scheme.  The OS query is heavily guarded so it never raises on
        platforms/Qt versions lacking ``colorScheme()`` (offscreen returns
        ``Unknown`` -> treated as light/False).
        """
        if mode == "dark":
            return True
        if mode == "light":
            return False
        # mode == "system": query the OS, falling back to light on any failure.
        try:
            from PySide6.QtCore import Qt
            from PySide6.QtWidgets import QApplication

            app = QApplication.instance()
            if app is None:
                return False
            hints = getattr(app, "styleHints", None)
            if hints is None:
                return False
            style_hints = hints()
            color_scheme = getattr(style_hints, "colorScheme", None)
            if color_scheme is None:
                return False
            return color_scheme() == Qt.ColorScheme.Dark
        except Exception:
            return False

    def set_mode(self, mode: str) -> None:
        """Switch appearance mode ("light"/"dark"/"system") and apply it."""
        if mode not in ("light", "dark", "system"):
            return
        self.mode = mode
        dark = self._resolve_mode_dark(mode)
        self.set_theme(dark)
