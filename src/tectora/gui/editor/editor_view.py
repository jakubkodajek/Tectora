# -*- coding: utf-8 -*-
"""EditorView — roof footprint editor shell (G2).

Layout (top-to-bottom):
  - Top bar   : ‹ Zpět | project name | menu items | lang chip
  - Toolbar   : tool buttons (Kreslit / Upravit body / Okap·Štít) |
                pitch button | 2D/3D toggle | ▶ Provést výpočet
  - Content   : canvas placeholder (left)  +  right panel (~300 px)
  - Status bar: cursor coords / units label

Public attributes consumed by G3+:
  self.active_tool       – str in {"draw", "editPoints", "eaveGable"}
  self.view_mode         – str in {"2d", "3d"}
  self.canvas_container  – QWidget  (G3 replaces its content)
  self.status_label      – QLabel   (G3 updates cursor coords)
  self._on_calculate()   – slot     (G4 fills in the logic)
"""
from __future__ import annotations

from typing import TYPE_CHECKING, Optional

from PySide6.QtCore import Qt, QTimer
from PySide6.QtGui import QFont
from PySide6.QtWidgets import (
    QButtonGroup,
    QFrame,
    QHBoxLayout,
    QLabel,
    QMenuBar,
    QPushButton,
    QScrollArea,
    QSizePolicy,
    QVBoxLayout,
    QWidget,
)

from tectora.gui.i18n import tr
from tectora.gui.theme import (
    ACCENT,
    ACCENT_HOVER,
    BG,
    BORDER,
    BORDER_SOFT,
    CARD,
    FAINT,
    MID,
    MUTED,
    TEXT,
)

if TYPE_CHECKING:
    pass  # MainWindow imported at runtime only


# ---------------------------------------------------------------------------
# Small helpers
# ---------------------------------------------------------------------------

_DEFAULT_TITLE = "Nová zakázka"

# Tool identifiers match i18n keys
_TOOLS = ("draw", "editPoints", "eaveGable")


def _tool_btn(label: str, checkable: bool = True) -> QPushButton:
    btn = QPushButton(label)
    btn.setCheckable(checkable)
    btn.setFixedHeight(34)
    btn.setSizePolicy(QSizePolicy.Policy.Preferred, QSizePolicy.Policy.Fixed)
    btn.setStyleSheet(
        f"QPushButton {{"
        f"  background: {CARD};"
        f"  color: {MID};"
        f"  border: 1px solid {BORDER};"
        f"  border-radius: 7px;"
        f"  padding: 0 14px;"
        f"  font-size: 10pt;"
        f"  font-weight: 500;"
        f"}}"
        f"QPushButton:checked {{"
        f"  background: {TEXT};"
        f"  color: #ffffff;"
        f"  border-color: {TEXT};"
        f"}}"
        f"QPushButton:hover:!checked {{"
        f"  background: {BG};"
        f"}}"
    )
    return btn


def _section_header(text: str) -> QLabel:
    lbl = QLabel(text)
    lbl.setStyleSheet(
        f"color: {FAINT}; font-size: 8pt; font-weight: 600;"
        f" letter-spacing: 1px; background: transparent;"
    )
    return lbl


# ---------------------------------------------------------------------------
# EditorView
# ---------------------------------------------------------------------------

class EditorView(QWidget):
    """Full-screen editor shell (G2)."""

    # Public for G3+
    active_tool: str = "draw"
    view_mode: str = "2d"

    def __init__(self, window, parent: QWidget | None = None):
        super().__init__(parent)
        self._window = window
        self._project = None
        self._entry_id = None
        self.pitch: int = 35          # current pitch in degrees; G5 will wire the control
        self.last_result = None       # last successful RoofResult or None
        self._build_ui()

    # ------------------------------------------------------------------
    # Public interface
    # ------------------------------------------------------------------

    def load_project(self, project, entry_id=None) -> None:
        """Store project, update title. Called by MainWindow.open_editor()."""
        self._project = project
        self._entry_id = entry_id
        name = (project.project_name or "").strip() if project else ""
        self._title_label.setText(name or _DEFAULT_TITLE)
        # Sync pitch button if project is available
        if project is not None:
            pitch = int(round(project.global_pitch_deg))
            self._pitch_btn.setText(f"{tr('pitch', self._lang)}  {pitch}°  ▾")

    # ------------------------------------------------------------------
    # UI construction
    # ------------------------------------------------------------------

    def _build_ui(self) -> None:
        self._lang = getattr(self._window, "lang", "cs")

        root = QVBoxLayout(self)
        root.setContentsMargins(0, 0, 0, 0)
        root.setSpacing(0)

        root.addWidget(self._make_topbar())
        root.addWidget(self._make_toolbar())
        root.addWidget(self._make_content(), stretch=1)
        root.addWidget(self._make_statusbar())

    # ── Top bar ──────────────────────────────────────────────────────────

    def _make_topbar(self) -> QWidget:
        bar = QWidget()
        bar.setFixedHeight(52)
        bar.setStyleSheet(
            f"background: {CARD}; border-bottom: 1px solid {BORDER_SOFT};"
        )
        layout = QHBoxLayout(bar)
        layout.setContentsMargins(12, 0, 16, 0)
        layout.setSpacing(0)

        # ‹ Zpět button
        back_btn = QPushButton(f"‹  {tr('back', self._lang)}")
        back_btn.setFixedHeight(34)
        back_btn.setStyleSheet(
            f"QPushButton {{"
            f"  background: transparent;"
            f"  color: {MID};"
            f"  border: none;"
            f"  padding: 0 10px 0 6px;"
            f"  font-size: 10pt;"
            f"  font-weight: 500;"
            f"}}"
            f"QPushButton:hover {{ color: {TEXT}; }}"
        )
        back_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        back_btn.clicked.connect(self._window.open_home)
        layout.addWidget(back_btn)

        layout.addSpacing(12)

        # Project name label
        self._title_label = QLabel(_DEFAULT_TITLE)
        tf = QFont()
        tf.setWeight(QFont.Weight.DemiBold)
        tf.setPointSize(11)
        self._title_label.setFont(tf)
        self._title_label.setStyleSheet(
            f"color: {TEXT}; background: transparent;"
        )
        layout.addWidget(self._title_label)

        layout.addSpacing(32)

        # Menu bar (File / Edit / View / Help)
        menu_bar = QMenuBar()
        menu_bar.setSizePolicy(QSizePolicy.Policy.Preferred, QSizePolicy.Policy.Fixed)
        menu_bar.setStyleSheet(
            f"QMenuBar {{ background: transparent; border: none; color: {MID}; }}"
            f"QMenuBar::item {{ padding: 6px 10px; border-radius: 4px; }}"
            f"QMenuBar::item:selected {{ background: {BG}; }}"
        )
        for key in ("file", "edit", "view", "help"):
            menu = menu_bar.addMenu(tr(key, self._lang))
            # Stub actions — G-later will populate
            menu.addAction(f"({tr(key, self._lang)}…)")
        layout.addWidget(menu_bar)

        layout.addStretch(1)

        # Language chip
        lang_chip = QPushButton(f"{self._lang.upper()} ▾")
        lang_chip.setFixedHeight(32)
        lang_chip.setStyleSheet(
            f"QPushButton {{"
            f"  background: {CARD};"
            f"  color: {MID};"
            f"  border: 1px solid {BORDER};"
            f"  border-radius: 8px;"
            f"  font-size: 9pt;"
            f"  font-weight: 600;"
            f"  padding: 0 12px;"
            f"}}"
            f"QPushButton:hover {{ background: {BG}; }}"
        )
        layout.addWidget(lang_chip)

        return bar

    # ── Toolbar ──────────────────────────────────────────────────────────

    def _make_toolbar(self) -> QWidget:
        bar = QWidget()
        bar.setFixedHeight(52)
        bar.setStyleSheet(
            f"background: {CARD}; border-bottom: 1px solid {BORDER_SOFT};"
        )
        layout = QHBoxLayout(bar)
        layout.setContentsMargins(16, 0, 16, 0)
        layout.setSpacing(8)

        # Tool button group (mutually exclusive)
        self._tool_group = QButtonGroup(self)
        self._tool_group.setExclusive(True)

        tool_keys = {
            "draw": tr("draw", self._lang),
            "editPoints": tr("editPoints", self._lang),
            "eaveGable": tr("eaveGable", self._lang),
        }
        self._tool_buttons: dict[str, QPushButton] = {}
        for key, label in tool_keys.items():
            btn = _tool_btn(label)
            self._tool_group.addButton(btn)
            layout.addWidget(btn)
            self._tool_buttons[key] = btn
            btn.clicked.connect(lambda _checked, k=key: self._set_active_tool(k))

        # Activate the first tool
        first_btn = self._tool_buttons["draw"]
        first_btn.setChecked(True)
        self.active_tool = "draw"

        layout.addSpacing(12)

        # Separator
        sep = QFrame()
        sep.setFrameShape(QFrame.Shape.VLine)
        sep.setStyleSheet(f"color: {BORDER};")
        layout.addWidget(sep)

        layout.addSpacing(12)

        # Pitch button (stub — shows current pitch; G5 will open a popover)
        pitch = 35
        self._pitch_btn = QPushButton(f"{tr('pitch', self._lang)}  {pitch}°  ▾")
        self._pitch_btn.setFixedHeight(34)
        self._pitch_btn.setStyleSheet(
            f"QPushButton {{"
            f"  background: {CARD};"
            f"  color: {MID};"
            f"  border: 1px solid {BORDER};"
            f"  border-radius: 7px;"
            f"  padding: 0 14px;"
            f"  font-size: 10pt;"
            f"}}"
            f"QPushButton:hover {{ background: {BG}; }}"
        )
        layout.addWidget(self._pitch_btn)

        layout.addStretch(1)

        # 2D / 3D segmented toggle
        seg_container = QWidget()
        seg_container.setFixedHeight(36)
        seg_container.setStyleSheet(
            f"background: {BG}; border-radius: 9px;"
        )
        seg_layout = QHBoxLayout(seg_container)
        seg_layout.setContentsMargins(3, 3, 3, 3)
        seg_layout.setSpacing(2)

        self._btn_2d = QPushButton("2D")
        self._btn_2d.setCheckable(True)
        self._btn_2d.setChecked(True)
        self._btn_2d.setFixedSize(48, 30)
        self._btn_3d = QPushButton("3D")
        self._btn_3d.setCheckable(True)
        self._btn_3d.setFixedSize(48, 30)

        for b in (self._btn_2d, self._btn_3d):
            b.setStyleSheet(
                f"QPushButton {{"
                f"  background: transparent;"
                f"  color: {MUTED};"
                f"  border: none;"
                f"  border-radius: 7px;"
                f"  font-size: 10pt;"
                f"  font-weight: 600;"
                f"}}"
                f"QPushButton:checked {{"
                f"  background: {CARD};"
                f"  color: {TEXT};"
                f"}}"
            )
            seg_layout.addWidget(b)

        self._view_group = QButtonGroup(self)
        self._view_group.setExclusive(True)
        self._view_group.addButton(self._btn_2d)
        self._view_group.addButton(self._btn_3d)
        self._btn_2d.clicked.connect(lambda: self._set_view_mode("2d"))
        self._btn_3d.clicked.connect(lambda: self._set_view_mode("3d"))
        self.view_mode = "2d"

        layout.addWidget(seg_container)
        layout.addSpacing(12)

        # ▶ Provést výpočet (primary action)
        self._calc_btn = QPushButton(f"▶  {tr('calc', self._lang)}")
        self._calc_btn.setFixedHeight(36)
        self._calc_btn.setStyleSheet(
            f"QPushButton {{"
            f"  background: {ACCENT};"
            f"  color: #ffffff;"
            f"  border: none;"
            f"  border-radius: 8px;"
            f"  padding: 0 20px;"
            f"  font-size: 10pt;"
            f"  font-weight: 600;"
            f"}}"
            f"QPushButton:hover {{ background: {ACCENT_HOVER}; }}"
            f"QPushButton:pressed {{ background: {ACCENT_HOVER}; }}"
        )
        self._calc_btn.clicked.connect(self._on_calculate)
        layout.addWidget(self._calc_btn)

        return bar

    # ── Content area (canvas + right panel) ──────────────────────────────

    def _make_content(self) -> QWidget:
        content = QWidget()
        content.setStyleSheet(f"background: {BG};")
        h_layout = QHBoxLayout(content)
        h_layout.setContentsMargins(0, 0, 0, 0)
        h_layout.setSpacing(0)

        # Canvas container — G3 fills this with Canvas2D
        self.canvas_container = QWidget()
        self.canvas_container.setSizePolicy(
            QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding
        )
        self.canvas_container.setStyleSheet(
            f"background: #f0f4f8;"
        )
        _inner = QVBoxLayout(self.canvas_container)
        _inner.setContentsMargins(0, 0, 0, 0)
        _inner.setSpacing(0)

        h_layout.addWidget(self.canvas_container, stretch=1)

        # G3: instantiate Canvas2D and fill canvas_container
        from tectora.gui.editor.canvas_2d import Canvas2D
        self.canvas = Canvas2D(self)
        self.canvas.setSizePolicy(
            QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding
        )
        _inner.addWidget(self.canvas)

        # G4b: wire live re-generation on footprint edits
        self.canvas.edited.connect(self._on_footprint_edited)

        # Right panel
        h_layout.addWidget(self._make_right_panel())

        return content

    # ── Right panel ───────────────────────────────────────────────────────

    def _make_right_panel(self) -> QWidget:
        panel = QWidget()
        panel.setFixedWidth(300)
        panel.setStyleSheet(
            f"background: {CARD}; border-left: 1px solid {BORDER_SOFT};"
        )

        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setFrameShape(QFrame.Shape.NoFrame)
        scroll.setStyleSheet("background: transparent;")
        scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)

        inner = QWidget()
        inner.setStyleSheet(f"background: {CARD};")
        v = QVBoxLayout(inner)
        v.setContentsMargins(16, 20, 16, 20)
        v.setSpacing(0)

        lang = self._lang

        # ── ZAKÁZKA section ──────────────────────────────────────────
        v.addWidget(_section_header(tr("order", lang).upper()))
        v.addSpacing(10)

        for field_key in ("name", "location", "note"):
            lbl = QLabel(tr(field_key, lang))
            lbl.setStyleSheet(f"color: {MID}; font-size: 9pt; background: transparent;")
            v.addWidget(lbl)
            v.addSpacing(4)
            ph = QWidget()
            ph.setFixedHeight(36)
            ph.setStyleSheet(
                f"background: {CARD}; border: 1px solid {BORDER}; border-radius: 8px;"
            )
            v.addWidget(ph)
            v.addSpacing(12)

        v.addSpacing(8)

        # ── PARAMETRY section ─────────────────────────────────────────
        v.addWidget(_section_header(tr("params", lang).upper()))
        v.addSpacing(10)

        for field_key in ("pitch", "roofType"):
            lbl = QLabel(tr(field_key, lang))
            lbl.setStyleSheet(f"color: {MID}; font-size: 9pt; background: transparent;")
            v.addWidget(lbl)
            v.addSpacing(4)
            ph = QWidget()
            ph.setFixedHeight(36)
            ph.setStyleSheet(
                f"background: {CARD}; border: 1px solid {BORDER}; border-radius: 8px;"
            )
            v.addWidget(ph)
            v.addSpacing(12)

        v.addSpacing(8)

        # ── VÝSLEDKY section ─────────────────────────────────────────
        v.addWidget(_section_header(tr("results", lang).upper()))
        v.addSpacing(10)

        # Result rows: (i18n_key, attr_name_or_None, unit_suffix)
        _RESULT_ROWS = [
            ("area",     "total_area_m2",       " m²"),
            ("ridgeLen", "ridge_length_m",       " m"),
            ("hips",     "_hips_label",          ""),
            ("valleys",  "_valleys_label",       ""),
            ("eavePerim","eaves_perimeter_m",    " m"),
            ("rakeL",    "gable_edge_left_m",    " m"),
            ("rakeR",    "gable_edge_right_m",   " m"),
        ]

        self._result_value_labels: dict[str, QLabel] = {}

        results_box = QWidget()
        results_box.setStyleSheet(
            f"background: {BG}; border-radius: 8px;"
        )
        rb_layout = QVBoxLayout(results_box)
        rb_layout.setContentsMargins(10, 10, 10, 10)
        rb_layout.setSpacing(6)

        for row_key, _attr, _unit in _RESULT_ROWS:
            row_w = QWidget()
            row_w.setStyleSheet("background: transparent;")
            row_h = QHBoxLayout(row_w)
            row_h.setContentsMargins(0, 0, 0, 0)
            row_h.setSpacing(4)

            lbl = QLabel(tr(row_key, lang))
            lbl.setStyleSheet(
                f"color: {MID}; font-size: 9pt; background: transparent;"
            )
            row_h.addWidget(lbl, stretch=1)

            val = QLabel("—")
            val.setStyleSheet(
                f"color: {TEXT}; font-size: 9pt; font-weight: 600;"
                f" background: transparent;"
            )
            val.setAlignment(Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter)
            row_h.addWidget(val)
            self._result_value_labels[row_key] = val
            rb_layout.addWidget(row_w)

        v.addWidget(results_box)

        v.addStretch(1)

        scroll.setWidget(inner)

        outer = QVBoxLayout(panel)
        outer.setContentsMargins(0, 0, 0, 0)
        outer.setSpacing(0)
        outer.addWidget(scroll)

        return panel

    # ── Status bar ────────────────────────────────────────────────────────

    def _make_statusbar(self) -> QWidget:
        bar = QWidget()
        bar.setFixedHeight(28)
        bar.setStyleSheet(
            f"background: {CARD}; border-top: 1px solid {BORDER_SOFT};"
        )
        layout = QHBoxLayout(bar)
        layout.setContentsMargins(16, 0, 16, 0)

        self.status_label = QLabel(f"Kurzor —  ·  Jednotky: m")
        self.status_label.setStyleSheet(
            f"color: {MUTED}; font-size: 9pt; background: transparent;"
        )
        layout.addWidget(self.status_label)
        layout.addStretch(1)

        return bar

    # ------------------------------------------------------------------
    # Slots / internal helpers
    # ------------------------------------------------------------------

    def _set_active_tool(self, key: str) -> None:
        self.active_tool = key

    def _set_view_mode(self, mode: str) -> None:
        self.view_mode = mode

    def _on_calculate(self) -> None:
        """Provést výpočet — compute the roof and update canvas + results panel."""
        from tectora.application import use_cases as uc
        from tectora.application.project import Project

        lang = self._lang
        fp = self.canvas.footprint
        if fp is None:
            self._show_toast(tr("drawFirst", lang))
            return
        try:
            result = uc.calculate_project(
                Project(
                    project_name=(self._project.project_name if self._project else ""),
                    footprint=fp,
                    global_pitch_deg=float(self.pitch),
                )
            )
        except ValueError:
            self._show_toast(tr("unsupportedShape", lang))
            return

        self.last_result = result
        self.canvas.set_skeleton(result)
        self.set_results(result)

    def set_results(self, result) -> None:
        """Populate the VÝSLEDKY rows from *result* (a RoofResult)."""
        labels = self._result_value_labels
        if "area" in labels:
            labels["area"].setText(f"{result.total_area_m2:.2f} m²")
        if "ridgeLen" in labels:
            labels["ridgeLen"].setText(f"{result.ridge_length_m:.2f} m")
        if "hips" in labels:
            labels["hips"].setText(
                f"{result.hip_count} ks / {result.hip_length_m:.2f} m"
            )
        if "valleys" in labels:
            vcount = len([
                s for s in getattr(result, "faces", [])
                if "valley" in getattr(s, "name", "").lower()
            ]) if result.valley_length_m > 0 else 0
            labels["valleys"].setText(f"{result.valley_length_m:.2f} m")
        if "eavePerim" in labels:
            labels["eavePerim"].setText(f"{result.eaves_perimeter_m:.2f} m")
        if "rakeL" in labels:
            labels["rakeL"].setText(f"{result.gable_edge_left_m:.2f} m")
        if "rakeR" in labels:
            labels["rakeR"].setText(f"{result.gable_edge_right_m:.2f} m")

    def clear_results(self) -> None:
        """Reset all VÝSLEDKY rows to dashes."""
        for lbl in self._result_value_labels.values():
            lbl.setText("—")
        self.last_result = None

    # ------------------------------------------------------------------
    # G4b: live re-generation
    # ------------------------------------------------------------------

    def _on_footprint_edited(self) -> None:
        """Re-run the calculation when the closed polygon is edited in place."""
        if self.last_result is not None and self.canvas.footprint is not None:
            self._on_calculate()

    def set_pitch(self, value: int) -> None:
        """Update the pitch, refresh the button label, and recompute if a result exists."""
        self.pitch = int(value)
        self._pitch_btn.setText(f"{tr('pitch', self._lang)}  {self.pitch}°  ▾")
        if self.last_result is not None and self.canvas.footprint is not None:
            self._on_calculate()

    def _show_toast(self, text: str) -> None:
        """Show a dark floating toast message that auto-hides after ~2.5 s."""
        toast = QLabel(text, self)
        toast.setStyleSheet(
            "background: #111827;"
            " color: #ffffff;"
            " border-radius: 8px;"
            " padding: 10px 16px;"
            " font-size: 10pt;"
            " font-weight: 500;"
        )
        toast.setAlignment(Qt.AlignmentFlag.AlignCenter)
        toast.adjustSize()
        # Centre horizontally, 80 px from bottom
        pw, ph = self.width(), self.height()
        tw, th = toast.width() + 32, toast.height() + 4
        toast.resize(tw, th)
        toast.move((pw - tw) // 2, ph - th - 80)
        toast.raise_()
        toast.show()
        QTimer.singleShot(2500, toast.deleteLater)
