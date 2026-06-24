# -*- coding: utf-8 -*-
"""LocationField — a QLineEdit with a debounced location-autocomplete dropdown.

Design notes
------------
* Debounce: a 300 ms QTimer fires after the user stops typing; the search is
  then executed on a background QThread so the UI thread never blocks.
* Offline / empty query: suggest_location already returns [] gracefully.
* Synchronous test helpers (no threads):
    _run_search(text)          -> list[Location]
    set_selected(location)     -> sets self.location + fills the line edit
    set_text(text) / text()    -> forward to the internal QLineEdit
    self.location              -> None | Location
"""
from __future__ import annotations

from typing import Optional

from PySide6.QtCore import Qt, QThread, QTimer, Signal
from PySide6.QtWidgets import (
    QHBoxLayout,
    QLabel,
    QListWidget,
    QListWidgetItem,
    QVBoxLayout,
    QLineEdit,
    QWidget,
)

from tectora.application.use_cases import suggest_location
from tectora.core.location import Location
from tectora.gui import theme


# ---------------------------------------------------------------------------
# Background worker
# ---------------------------------------------------------------------------

class _SearchWorker(QThread):
    """Runs suggest_location off the UI thread and emits results."""

    results_ready = Signal(list)  # list[Location]

    def __init__(self, geocoder, query: str, parent=None):
        super().__init__(parent)
        self._geocoder = geocoder
        self._query = query

    def run(self) -> None:
        locs = suggest_location(self._geocoder, self._query)
        self.results_ready.emit(locs)


# ---------------------------------------------------------------------------
# LocationField widget
# ---------------------------------------------------------------------------

class LocationField(QWidget):
    """A line-edit with pin icon and an autocomplete results dropdown."""

    # Emitted when the user selects a location
    location_selected = Signal(object)  # Location

    def __init__(
        self,
        geocoder=None,
        lang: str = "cs",
        parent: QWidget | None = None,
    ):
        super().__init__(parent)
        if geocoder is None:
            from tectora.io.nominatim_geocoder import NominatimGeocoder
            geocoder = NominatimGeocoder()
        self._geocoder = geocoder
        self._lang = lang
        self.location: Optional[Location] = None
        self._worker: Optional[_SearchWorker] = None

        self._build_ui()
        self._debounce = QTimer(self)
        self._debounce.setSingleShot(True)
        self._debounce.setInterval(300)
        self._debounce.timeout.connect(self._launch_search)

    # ------------------------------------------------------------------
    # UI
    # ------------------------------------------------------------------

    def _build_ui(self) -> None:
        outer = QVBoxLayout(self)
        outer.setContentsMargins(0, 0, 0, 0)
        outer.setSpacing(2)

        # Row: pin + line edit
        row = QWidget()
        row.setStyleSheet(
            f"background: {theme.CARD}; border: 1px solid {theme.BORDER}; border-radius: 8px;"
        )
        row_layout = QHBoxLayout(row)
        row_layout.setContentsMargins(8, 0, 8, 0)
        row_layout.setSpacing(4)

        pin = QLabel("📍")
        pin.setStyleSheet("background: transparent; border: none; font-size: 10pt;")
        row_layout.addWidget(pin)

        self._line_edit = QLineEdit()
        self._line_edit.setPlaceholderText("Zadejte obec nebo adresu…")
        self._line_edit.setStyleSheet(
            "background: transparent; border: none; color: "
            f"{theme.TEXT}; font-size: 10pt;"
        )
        self._line_edit.setFixedHeight(34)
        row_layout.addWidget(self._line_edit, stretch=1)

        outer.addWidget(row)

        # Dropdown list (hidden until results arrive)
        self._dropdown = QListWidget()
        self._dropdown.setStyleSheet(
            f"QListWidget {{"
            f"  background: {theme.CARD};"
            f"  border: 1px solid {theme.BORDER};"
            f"  border-radius: 6px;"
            f"  font-size: 10pt;"
            f"  color: {theme.TEXT};"
            f"}}"
            f"QListWidget::item:selected {{"
            f"  background: {theme.BG};"
            f"  color: {theme.TEXT};"
            f"}}"
            f"QListWidget::item:hover {{"
            f"  background: {theme.BG};"
            f"}}"
        )
        self._dropdown.hide()
        self._dropdown.itemClicked.connect(self._on_item_clicked)
        outer.addWidget(self._dropdown)

        # Wire debounce
        self._line_edit.textChanged.connect(self._on_text_changed)

    # ------------------------------------------------------------------
    # Public test/API helpers
    # ------------------------------------------------------------------

    def _run_search(self, text: str) -> list[Location]:
        """Synchronous search (no threads). Used in tests."""
        return suggest_location(self._geocoder, text)

    def set_selected(self, location: Location) -> None:
        """Set the current location (and fill the line edit)."""
        self.location = location
        self._line_edit.blockSignals(True)
        self._line_edit.setText(location.name)
        self._line_edit.blockSignals(False)
        self._dropdown.hide()
        self.location_selected.emit(location)

    def set_text(self, text: str) -> None:
        self._line_edit.setText(text)

    def text(self) -> str:
        return self._line_edit.text()

    # ------------------------------------------------------------------
    # Internal slots
    # ------------------------------------------------------------------

    def _on_text_changed(self, text: str) -> None:
        self.location = None  # cleared whenever user types
        if text.strip():
            self._debounce.start()
        else:
            self._debounce.stop()
            self._dropdown.hide()

    def _launch_search(self) -> None:
        query = self._line_edit.text().strip()
        if not query:
            self._dropdown.hide()
            return
        # Cancel previous worker if still running
        if self._worker is not None and self._worker.isRunning():
            self._worker.quit()
            self._worker.wait()
        self._worker = _SearchWorker(self._geocoder, query, parent=self)
        self._worker.results_ready.connect(self._on_results)
        self._worker.start()

    def _on_results(self, locations: list[Location]) -> None:
        self._dropdown.clear()
        if not locations:
            self._dropdown.hide()
            return
        for loc in locations:
            item = QListWidgetItem(loc.name)
            item.setData(Qt.ItemDataRole.UserRole, loc)
            self._dropdown.addItem(item)
        self._dropdown.setFixedHeight(min(len(locations), 5) * 36)
        self._dropdown.show()

    def _on_item_clicked(self, item: QListWidgetItem) -> None:
        loc: Location = item.data(Qt.ItemDataRole.UserRole)
        self.set_selected(loc)
