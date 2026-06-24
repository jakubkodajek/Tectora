# -*- coding: utf-8 -*-
"""SettingsDialog — language selector + appearance mode selector (F2)."""
from __future__ import annotations

from PySide6.QtCore import Qt
from PySide6.QtWidgets import (
    QComboBox,
    QDialog,
    QDialogButtonBox,
    QFormLayout,
    QVBoxLayout,
    QWidget,
)

from tectora.gui.i18n import LANGS, tr
from tectora.gui import theme


class SettingsDialog(QDialog):
    """Modal settings dialog: language picker + appearance mode selector."""

    def __init__(
        self,
        current_lang: str = "cs",
        current_mode: str = "light",
        parent: QWidget | None = None,
    ):
        super().__init__(parent)
        self._lang = current_lang
        self.setWindowTitle(tr("settings", current_lang))
        self.setMinimumWidth(320)
        self._build_ui(current_lang, current_mode)

    # ------------------------------------------------------------------
    # UI
    # ------------------------------------------------------------------

    def _build_ui(self, current_lang: str, current_mode: str) -> None:
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

        # Appearance mode combo (light / dark / system)
        self._mode_combo = QComboBox()
        self._mode_combo.addItem(tr("modeLight", current_lang), "light")
        self._mode_combo.addItem(tr("modeDark", current_lang), "dark")
        self._mode_combo.addItem(tr("modeSystem", current_lang), "system")
        mode_idx = self._mode_combo.findData(current_mode)
        if mode_idx >= 0:
            self._mode_combo.setCurrentIndex(mode_idx)

        form.addRow(tr("mode", current_lang) + ":", self._mode_combo)

        layout.addLayout(form)

        # OK / Cancel buttons
        buttons = QDialogButtonBox(
            QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel
        )
        buttons.accepted.connect(self.accept)
        buttons.rejected.connect(self.reject)

        ok_btn = buttons.button(QDialogButtonBox.StandardButton.Ok)
        ok_btn.setProperty("primary", True)
        self._ok_btn = ok_btn

        layout.addWidget(buttons)

    # ------------------------------------------------------------------
    # Public API
    # ------------------------------------------------------------------

    def selected_lang(self) -> str:
        """Return the language code chosen by the user."""
        return self._lang_combo.currentData()

    def selected_mode(self) -> str:
        """Return the appearance mode code chosen by the user."""
        return self._mode_combo.currentData()

    def set_choices(self, lang: str, mode: str) -> None:
        """Programmatically set choices (used by tests)."""
        idx = list(LANGS).index(lang) if lang in LANGS else 0
        self._lang_combo.setCurrentIndex(idx)
        mode_idx = self._mode_combo.findData(mode)
        if mode_idx >= 0:
            self._mode_combo.setCurrentIndex(mode_idx)
