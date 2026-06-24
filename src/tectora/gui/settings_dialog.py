# -*- coding: utf-8 -*-
"""SettingsDialog — language selector + dark-theme toggle (F2)."""
from __future__ import annotations

from PySide6.QtCore import Qt
from PySide6.QtWidgets import (
    QCheckBox,
    QComboBox,
    QDialog,
    QDialogButtonBox,
    QFormLayout,
    QLabel,
    QVBoxLayout,
    QWidget,
)

from tectora.gui.i18n import LANGS, tr
from tectora.gui import theme


class SettingsDialog(QDialog):
    """Modal settings dialog: language picker + dark-theme checkbox."""

    def __init__(
        self,
        current_lang: str = "cs",
        current_dark: bool = False,
        parent: QWidget | None = None,
    ):
        super().__init__(parent)
        self._lang = current_lang
        self.setWindowTitle(tr("settings", current_lang))
        self.setMinimumWidth(320)
        self._build_ui(current_lang, current_dark)

    # ------------------------------------------------------------------
    # UI
    # ------------------------------------------------------------------

    def _build_ui(self, current_lang: str, current_dark: bool) -> None:
        layout = QVBoxLayout(self)
        layout.setContentsMargins(20, 16, 20, 16)
        layout.setSpacing(12)

        form = QFormLayout()
        form.setLabelAlignment(Qt.AlignmentFlag.AlignRight)
        form.setSpacing(10)

        # Language combo
        self._lang_combo = QComboBox()
        for code in LANGS:
            self._lang_combo.addItem(code.upper(), code)
        # Select current language
        idx = list(LANGS).index(current_lang) if current_lang in LANGS else 0
        self._lang_combo.setCurrentIndex(idx)

        form.addRow(tr("language", current_lang) + ":", self._lang_combo)

        # Dark theme checkbox
        self._dark_check = QCheckBox()
        self._dark_check.setChecked(current_dark)

        form.addRow(tr("darkTheme", current_lang) + ":", self._dark_check)

        layout.addLayout(form)

        # OK / Cancel buttons
        buttons = QDialogButtonBox(
            QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel
        )
        buttons.accepted.connect(self.accept)
        buttons.rejected.connect(self.reject)
        layout.addWidget(buttons)

    # ------------------------------------------------------------------
    # Public API
    # ------------------------------------------------------------------

    def selected_lang(self) -> str:
        """Return the language code chosen by the user."""
        return self._lang_combo.currentData()

    def selected_dark(self) -> bool:
        """Return True if the dark-theme checkbox is checked."""
        return self._dark_check.isChecked()

    def set_choices(self, lang: str, dark: bool) -> None:
        """Programmatically set choices (used by tests)."""
        idx = list(LANGS).index(lang) if lang in LANGS else 0
        self._lang_combo.setCurrentIndex(idx)
        self._dark_check.setChecked(dark)
