# -*- coding: utf-8 -*-
"""EditorView — roof footprint editor shell (G2).

Layout (top-to-bottom):
  - Top bar   : QMenuBar (Soubor/Zobrazení/Nastavení/Nápověda) +
                nav row (‹ Zpět | project name)
  - Toolbar   : tool buttons (Kreslit / Upravit body / Okap·Štít) |
                2D/3D toggle | ▶ Provést výpočet
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
    QDoubleSpinBox,
    QFrame,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QMenu,
    QMenuBar,
    QPlainTextEdit,
    QPushButton,
    QScrollArea,
    QSizePolicy,
    QSpinBox,
    QStackedWidget,
    QVBoxLayout,
    QWidget,
)

from tectora.gui.i18n import tr
from tectora.gui import theme

if TYPE_CHECKING:
    pass  # MainWindow imported at runtime only


# ---------------------------------------------------------------------------
# Small helpers
# ---------------------------------------------------------------------------

_DEFAULT_TITLE = ""  # filled at runtime from tr("untitled", lang)

# Tool identifiers match i18n keys
_TOOLS = ("draw", "editPoints", "eaveGable")


def _tool_btn(label: str, checkable: bool = True) -> QPushButton:
    btn = QPushButton(label)
    btn.setCheckable(checkable)
    btn.setFixedHeight(34)
    btn.setSizePolicy(QSizePolicy.Policy.Preferred, QSizePolicy.Policy.Fixed)
    btn.setStyleSheet(
        f"QPushButton {{"
        f"  background: {theme.CARD};"
        f"  color: {theme.MID};"
        f"  border: 1px solid {theme.BORDER};"
        f"  border-radius: 7px;"
        f"  padding: 0 14px;"
        f"  font-size: 10pt;"
        f"  font-weight: 500;"
        f"}}"
        f"QPushButton:checked {{"
        f"  background: {theme.TEXT};"
        f"  color: #ffffff;"
        f"  border-color: {theme.TEXT};"
        f"}}"
        f"QPushButton:hover:!checked {{"
        f"  background: {theme.BG};"
        f"}}"
    )
    return btn


def _section_header(text: str) -> QLabel:
    lbl = QLabel(text)
    lbl.setStyleSheet(
        f"color: {theme.FAINT}; font-size: 8pt; font-weight: 600;"
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
        self._current_entry_id = None   # G6: tracks saved entry id (None = unsaved)
        self.pitch: int = 35          # current pitch in degrees; G5 will wire the control
        self.wall_height: float = 2.5  # wall height in metres
        self.overhang: float = 0.0    # eave overhang in metres (F3)
        self.last_result = None       # last successful RoofResult or None
        self._build_ui()

    # ------------------------------------------------------------------
    # Public interface
    # ------------------------------------------------------------------

    def load_project(self, project, entry_id=None, note: str = "", location=None) -> None:
        """Store project, update title and right-panel fields.

        Called by MainWindow.open_editor().
        ``note`` and ``location`` come from the library entry (G6 will pass them).
        """
        self._project = project
        self._entry_id = entry_id
        self._current_entry_id = entry_id  # G6: keep in sync
        name = (project.project_name or "").strip() if project else ""
        self._title_label.setText(name or _DEFAULT_TITLE)

        # Populate ZAKÁZKA fields
        self.name_edit.setText(name)
        self.note_edit.setPlainText(note or "")
        if location is not None:
            self.location_field.set_selected(location)
        else:
            self.location_field.set_text("")

        # Sync pitch, wall-height and overhang controls if project is available
        if project is not None:
            pitch = int(round(project.global_pitch_deg))
            self._pitch_spin.setValue(pitch)
            if hasattr(self, "_wall_height_spin"):
                wh = getattr(project, "wall_height_m", 2.5)
                self._wall_height_spin.blockSignals(True)
                self._wall_height_spin.setValue(float(wh))
                self._wall_height_spin.blockSignals(False)
                self.wall_height = float(wh)
            if hasattr(self, "_overhang_spin"):
                oh = getattr(project, "overhang_m", 0.0)
                self._overhang_spin.blockSignals(True)
                self._overhang_spin.setValue(float(oh))
                self._overhang_spin.blockSignals(False)
                self.overhang = float(oh)

    # ------------------------------------------------------------------
    # UI construction
    # ------------------------------------------------------------------

    def _build_ui(self) -> None:
        self._lang = getattr(self._window, "lang", "cs")
        global _DEFAULT_TITLE
        _DEFAULT_TITLE = tr("untitled", self._lang)

        root = QVBoxLayout(self)
        root.setContentsMargins(0, 0, 0, 0)
        root.setSpacing(0)

        root.addWidget(self._make_topbar())
        root.addWidget(self._make_toolbar())
        root.addWidget(self._make_content(), stretch=1)
        root.addWidget(self._make_statusbar())

    # ── Top bar ──────────────────────────────────────────────────────────

    def _make_topbar(self) -> QWidget:
        """Build a two-row top area: menu bar row + back/title row."""
        container = QWidget()
        container.setStyleSheet(
            f"background: {theme.CARD}; border-bottom: 1px solid {theme.BORDER_SOFT};"
        )
        vbox = QVBoxLayout(container)
        vbox.setContentsMargins(0, 0, 0, 0)
        vbox.setSpacing(0)

        # ── Row 1: real QMenuBar ──────────────────────────────────────
        lang = self._lang
        window = self._window

        self.menubar = QMenuBar()
        self.menubar.setStyleSheet(
            f"QMenuBar {{ background: {theme.CARD}; border: none; color: {theme.MID};"
            f" border-bottom: 1px solid {theme.BORDER_SOFT}; }}"
            f"QMenuBar::item {{ padding: 6px 12px; border-radius: 4px; }}"
            f"QMenuBar::item:selected {{ background: {theme.BG}; }}"
        )

        # Soubor
        from tectora.application.project import Project as _Project
        menu_file = self.menubar.addMenu(tr("file", lang))
        menu_file.addAction(tr("new", lang), lambda: window.open_editor(_Project(global_pitch_deg=35.0)))
        menu_file.addAction(tr("open", lang), window.open_home)
        menu_file.addAction(tr("save", lang), self._on_save)
        menu_file.addSeparator()
        menu_file.addAction(tr("openFile", lang), self._on_file_open)
        menu_file.addAction(tr("saveFile", lang), self._on_file_save)
        menu_file.addSeparator()
        menu_file.addAction(tr("exportXls", lang), self._on_export)
        menu_file.addSeparator()
        menu_file.addAction(tr("exit", lang), window.close)

        # Zobrazení
        menu_view = self.menubar.addMenu(tr("view", lang))
        menu_view.addAction(tr("view2d", lang), lambda: self._set_view_mode("2d"))
        menu_view.addAction(tr("view3d", lang), lambda: self._set_view_mode("3d"))
        menu_view.addSeparator()
        menu_view.addAction(tr("zoomIn", lang), lambda: self.canvas.zoom_in())
        menu_view.addAction(tr("zoomOut", lang), lambda: self.canvas.zoom_out())

        # Nastavení
        menu_settings = self.menubar.addMenu(tr("settings", lang))
        menu_settings.addAction(tr("settings", lang) + "…", self._open_settings)

        # Nápověda
        menu_help = self.menubar.addMenu(tr("help", lang))
        menu_help.addAction(tr("about", lang), self._on_about)

        vbox.addWidget(self.menubar)

        # ── Row 2: ‹ Zpět  |  project name ──────────────────────────
        nav_bar = QWidget()
        nav_bar.setFixedHeight(46)
        nav_bar.setStyleSheet("background: transparent;")
        nav_layout = QHBoxLayout(nav_bar)
        nav_layout.setContentsMargins(12, 0, 16, 0)
        nav_layout.setSpacing(0)

        back_btn = QPushButton(f"‹  {tr('back', lang)}")
        back_btn.setFixedHeight(34)
        back_btn.setStyleSheet(
            f"QPushButton {{"
            f"  background: transparent;"
            f"  color: {theme.MID};"
            f"  border: none;"
            f"  padding: 0 10px 0 6px;"
            f"  font-size: 10pt;"
            f"  font-weight: 500;"
            f"}}"
            f"QPushButton:hover {{ color: {theme.TEXT}; }}"
        )
        back_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        back_btn.clicked.connect(window.open_home)
        nav_layout.addWidget(back_btn)

        nav_layout.addSpacing(12)

        self._title_label = QLabel(_DEFAULT_TITLE)
        tf = QFont()
        tf.setWeight(QFont.Weight.DemiBold)
        tf.setPointSize(11)
        self._title_label.setFont(tf)
        self._title_label.setStyleSheet(
            f"color: {theme.TEXT}; background: transparent;"
        )
        nav_layout.addWidget(self._title_label)

        nav_layout.addStretch(1)

        vbox.addWidget(nav_bar)

        return container

    # ── Toolbar ──────────────────────────────────────────────────────────

    def _make_toolbar(self) -> QWidget:
        bar = QWidget()
        bar.setFixedHeight(52)
        bar.setStyleSheet(
            f"background: {theme.CARD}; border-bottom: 1px solid {theme.BORDER_SOFT};"
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
        sep.setStyleSheet(f"color: {theme.BORDER};")
        layout.addWidget(sep)

        layout.addStretch(1)

        # 2D / 3D segmented toggle
        seg_container = QWidget()
        seg_container.setFixedHeight(36)
        seg_container.setStyleSheet(
            f"background: {theme.BG}; border-radius: 9px;"
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
                f"  color: {theme.MUTED};"
                f"  border: none;"
                f"  border-radius: 7px;"
                f"  font-size: 10pt;"
                f"  font-weight: 600;"
                f"}}"
                f"QPushButton:checked {{"
                f"  background: {theme.CARD};"
                f"  color: {theme.TEXT};"
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
            f"  background: {theme.ACCENT};"
            f"  color: #ffffff;"
            f"  border: none;"
            f"  border-radius: 8px;"
            f"  padding: 0 20px;"
            f"  font-size: 10pt;"
            f"  font-weight: 600;"
            f"}}"
            f"QPushButton:hover {{ background: {theme.ACCENT_HOVER}; }}"
            f"QPushButton:pressed {{ background: {theme.ACCENT_HOVER}; }}"
        )
        self._calc_btn.clicked.connect(self._on_calculate)
        layout.addWidget(self._calc_btn)

        return bar

    # ── Content area (canvas + right panel) ──────────────────────────────

    def _make_content(self) -> QWidget:
        content = QWidget()
        content.setStyleSheet(f"background: {theme.BG};")
        h_layout = QHBoxLayout(content)
        h_layout.setContentsMargins(20, 0, 0, 0)
        h_layout.setSpacing(0)

        # Canvas container — holds a QStackedWidget with 2D (index 0) and 3D (index 1)
        self.canvas_container = QWidget()
        self.canvas_container.setSizePolicy(
            QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding
        )
        self.canvas_container.setStyleSheet(
            f"background: {theme.BG};"
        )
        _inner = QVBoxLayout(self.canvas_container)
        _inner.setContentsMargins(0, 0, 0, 0)
        _inner.setSpacing(0)

        h_layout.addWidget(self.canvas_container, stretch=1)

        # G3: instantiate Canvas2D
        from tectora.gui.editor.canvas_2d import Canvas2D
        self.canvas = Canvas2D(self)
        self.canvas.setSizePolicy(
            QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding
        )

        # G7: instantiate Roof3DView
        from tectora.gui.editor.view_3d import Roof3DView
        self.view3d = Roof3DView(self)
        self.view3d.setSizePolicy(
            QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding
        )

        # G7: stacked widget — index 0 = 2D canvas, index 1 = 3D view
        self._canvas_stack = QStackedWidget()
        self._canvas_stack.addWidget(self.canvas)   # index 0
        self._canvas_stack.addWidget(self.view3d)   # index 1
        self._canvas_stack.setCurrentIndex(0)
        _inner.addWidget(self._canvas_stack)

        # G4b: wire live re-generation on footprint edits
        self.canvas.edited.connect(self._on_footprint_edited)

        # Right panel
        h_layout.addWidget(self._make_right_panel())

        return content

    # ── Right panel ───────────────────────────────────────────────────────

    def _make_right_panel(self) -> QWidget:
        from tectora.gui.editor.location_field import LocationField

        panel = QWidget()
        panel.setFixedWidth(300)
        panel.setStyleSheet(
            f"background: {theme.CARD}; border-left: 1px solid {theme.BORDER_SOFT};"
        )
        self.right_panel = panel  # exposed for tests (test_dark_visible)

        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setFrameShape(QFrame.Shape.NoFrame)
        scroll.setStyleSheet("background: transparent;")
        scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)

        inner = QWidget()
        inner.setStyleSheet(f"background: {theme.CARD};")
        v = QVBoxLayout(inner)
        v.setContentsMargins(16, 20, 16, 20)
        v.setSpacing(0)

        lang = self._lang

        # ── ZAKÁZKA section ──────────────────────────────────────────
        v.addWidget(_section_header(tr("order", lang).upper()))
        v.addSpacing(10)

        # Název
        _lbl_name = QLabel(tr("name", lang))
        _lbl_name.setStyleSheet(f"color: {theme.MID}; font-size: 9pt; background: transparent;")
        v.addWidget(_lbl_name)
        v.addSpacing(4)
        self.name_edit = QLineEdit()
        self.name_edit.setPlaceholderText(tr("name", lang) + "…")
        self.name_edit.setFixedHeight(36)
        self.name_edit.setStyleSheet(
            f"background: {theme.CARD}; border: 1px solid {theme.BORDER}; border-radius: 8px;"
            f" padding: 0 10px; color: {theme.MID}; font-size: 10pt;"
        )
        v.addWidget(self.name_edit)
        v.addSpacing(12)

        # Poloha
        _lbl_loc = QLabel(tr("location", lang))
        _lbl_loc.setStyleSheet(f"color: {theme.MID}; font-size: 9pt; background: transparent;")
        v.addWidget(_lbl_loc)
        v.addSpacing(4)
        self.location_field = LocationField(lang=lang)
        v.addWidget(self.location_field)
        v.addSpacing(12)

        # Poznámka
        _lbl_note = QLabel(tr("note", lang))
        _lbl_note.setStyleSheet(f"color: {theme.MID}; font-size: 9pt; background: transparent;")
        v.addWidget(_lbl_note)
        v.addSpacing(4)
        self.note_edit = QPlainTextEdit()
        self.note_edit.setPlaceholderText("Volitelná poznámka…")
        self.note_edit.setFixedHeight(72)
        self.note_edit.setStyleSheet(
            f"background: {theme.CARD}; border: 1px solid {theme.BORDER}; border-radius: 8px;"
            f" padding: 6px 10px; color: {theme.MID}; font-size: 10pt;"
        )
        v.addWidget(self.note_edit)
        v.addSpacing(12)

        v.addSpacing(8)

        # ── PARAMETRY section ─────────────────────────────────────────
        v.addWidget(_section_header(tr("params", lang).upper()))
        v.addSpacing(10)

        # Sklon (pitch spin box with − / value / + layout)
        _lbl_pitch = QLabel(tr("pitch", lang))
        _lbl_pitch.setStyleSheet(f"color: {theme.MID}; font-size: 9pt; background: transparent;")
        v.addWidget(_lbl_pitch)
        v.addSpacing(4)

        pitch_row = QWidget()
        pitch_row.setStyleSheet("background: transparent;")
        pitch_layout = QHBoxLayout(pitch_row)
        pitch_layout.setContentsMargins(0, 0, 0, 0)
        pitch_layout.setSpacing(6)

        _btn_minus = QPushButton("−")
        _btn_minus.setFixedSize(32, 34)
        _btn_minus.setStyleSheet(
            f"QPushButton {{"
            f"  background: {theme.CARD}; color: {theme.MID}; border: 1px solid {theme.BORDER};"
            f"  border-radius: 7px; font-size: 14pt; font-weight: 600;"
            f"}}"
            f"QPushButton:hover {{ background: {theme.BG}; }}"
        )

        self._pitch_spin = QSpinBox()
        self._pitch_spin.setRange(1, 89)
        self._pitch_spin.setValue(self.pitch)
        self._pitch_spin.setSuffix("°")
        self._pitch_spin.setFixedHeight(34)
        self._pitch_spin.setStyleSheet(
            f"QSpinBox {{"
            f"  background: {theme.CARD}; color: {theme.MID}; border: 1px solid {theme.BORDER};"
            f"  border-radius: 7px; padding: 0 8px; font-size: 10pt;"
            f"  font-weight: 600;"
            f"}}"
            f"QSpinBox:focus {{ border-color: {theme.ACCENT}; }}"
            f"QSpinBox::up-button, QSpinBox::down-button {{ width: 0; }}"
        )

        _btn_plus = QPushButton("+")
        _btn_plus.setFixedSize(32, 34)
        _btn_plus.setStyleSheet(
            f"QPushButton {{"
            f"  background: {theme.CARD}; color: {theme.MID}; border: 1px solid {theme.BORDER};"
            f"  border-radius: 7px; font-size: 14pt; font-weight: 600;"
            f"}}"
            f"QPushButton:hover {{ background: {theme.BG}; }}"
        )

        _btn_minus.clicked.connect(self._pitch_minus)
        _btn_plus.clicked.connect(self._pitch_plus)
        self._pitch_spin.valueChanged.connect(self.set_pitch)

        pitch_layout.addWidget(_btn_minus)
        pitch_layout.addWidget(self._pitch_spin, stretch=1)
        pitch_layout.addWidget(_btn_plus)
        v.addWidget(pitch_row)
        v.addSpacing(12)

        # Výška stěn (wall height double spin box)
        _lbl_wall = QLabel(tr("wallHeight", lang))
        _lbl_wall.setStyleSheet(f"color: {theme.MID}; font-size: 9pt; background: transparent;")
        v.addWidget(_lbl_wall)
        v.addSpacing(4)

        self._wall_height_spin = QDoubleSpinBox()
        self._wall_height_spin.setRange(1.0, 10.0)
        self._wall_height_spin.setSingleStep(0.5)
        self._wall_height_spin.setDecimals(1)
        self._wall_height_spin.setValue(self.wall_height)
        self._wall_height_spin.setSuffix(" m")
        self._wall_height_spin.setFixedHeight(34)
        self._wall_height_spin.setStyleSheet(
            f"QDoubleSpinBox {{"
            f"  background: {theme.CARD}; color: {theme.MID}; border: 1px solid {theme.BORDER};"
            f"  border-radius: 7px; padding: 0 8px; font-size: 10pt;"
            f"  font-weight: 600;"
            f"}}"
            f"QDoubleSpinBox:focus {{ border-color: {theme.ACCENT}; }}"
            f"QDoubleSpinBox::up-button, QDoubleSpinBox::down-button {{ width: 0; }}"
        )
        self._wall_height_spin.valueChanged.connect(self._on_wall_height_changed)
        v.addWidget(self._wall_height_spin)
        v.addSpacing(12)

        # Přesah okapu (eave overhang double spin box)
        _lbl_overhang = QLabel(tr("overhang", lang))
        _lbl_overhang.setStyleSheet(f"color: {theme.MID}; font-size: 9pt; background: transparent;")
        v.addWidget(_lbl_overhang)
        v.addSpacing(4)

        self._overhang_spin = QDoubleSpinBox()
        self._overhang_spin.setRange(0.0, 2.0)
        self._overhang_spin.setSingleStep(0.1)
        self._overhang_spin.setDecimals(1)
        self._overhang_spin.setValue(self.overhang)
        self._overhang_spin.setSuffix(" m")
        self._overhang_spin.setFixedHeight(34)
        self._overhang_spin.setStyleSheet(
            f"QDoubleSpinBox {{"
            f"  background: {theme.CARD}; color: {theme.MID}; border: 1px solid {theme.BORDER};"
            f"  border-radius: 7px; padding: 0 8px; font-size: 10pt;"
            f"  font-weight: 600;"
            f"}}"
            f"QDoubleSpinBox:focus {{ border-color: {theme.ACCENT}; }}"
            f"QDoubleSpinBox::up-button, QDoubleSpinBox::down-button {{ width: 0; }}"
        )
        self._overhang_spin.valueChanged.connect(self._on_overhang_changed)
        v.addWidget(self._overhang_spin)
        v.addSpacing(12)

        # Typ střechy badge
        _lbl_roof_type = QLabel(tr("roofType", lang))
        _lbl_roof_type.setStyleSheet(f"color: {theme.MID}; font-size: 9pt; background: transparent;")
        v.addWidget(_lbl_roof_type)
        v.addSpacing(4)

        self.roof_type_badge = QLabel("—")
        self.roof_type_badge.setFixedHeight(32)
        self.roof_type_badge.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.roof_type_badge.setStyleSheet(
            f"background: {theme.BG}; color: {theme.ACCENT}; border-radius: 8px;"
            f" font-size: 10pt; font-weight: 600; padding: 0 12px;"
        )
        v.addWidget(self.roof_type_badge)
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
            f"background: {theme.BG}; border-radius: 8px;"
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
                f"color: {theme.MID}; font-size: 9pt; background: transparent;"
            )
            row_h.addWidget(lbl, stretch=1)

            val = QLabel("—")
            val.setStyleSheet(
                f"color: {theme.TEXT}; font-size: 9pt; font-weight: 600;"
                f" background: transparent;"
            )
            val.setAlignment(Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter)
            row_h.addWidget(val)
            self._result_value_labels[row_key] = val
            rb_layout.addWidget(row_w)

        v.addWidget(results_box)
        v.addSpacing(16)

        # ── Save button ───────────────────────────────────────────────
        self._save_btn = QPushButton(tr("save", lang))
        self._save_btn.setFixedHeight(40)
        self._save_btn.setStyleSheet(
            f"QPushButton {{"
            f"  background: {theme.ACCENT};"
            f"  color: #ffffff;"
            f"  border: none;"
            f"  border-radius: 9px;"
            f"  padding: 0 20px;"
            f"  font-size: 10pt;"
            f"  font-weight: 600;"
            f"}}"
            f"QPushButton:hover {{ background: {theme.ACCENT_HOVER}; }}"
        )
        self._save_btn.clicked.connect(self._on_save)
        v.addWidget(self._save_btn)

        # ── Export to Excel button ────────────────────────────────────
        self._export_btn = QPushButton(tr("exportXls", lang))
        self._export_btn.setFixedHeight(40)
        self._export_btn.setStyleSheet(
            f"QPushButton {{"
            f"  background: {theme.CARD};"
            f"  color: {theme.ACCENT};"
            f"  border: 1px solid {theme.ACCENT};"
            f"  border-radius: 9px;"
            f"  padding: 0 20px;"
            f"  font-size: 10pt;"
            f"  font-weight: 600;"
            f"}}"
            f"QPushButton:hover {{ background: {theme.BG}; }}"
        )
        self._export_btn.clicked.connect(self._on_export)
        v.addWidget(self._export_btn)

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
            f"background: {theme.CARD}; border-top: 1px solid {theme.BORDER_SOFT};"
        )
        layout = QHBoxLayout(bar)
        layout.setContentsMargins(16, 0, 16, 0)

        self.status_label = QLabel(f"Kurzor —  ·  Jednotky: m")
        self.status_label.setStyleSheet(
            f"color: {theme.MUTED}; font-size: 9pt; background: transparent;"
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
        if hasattr(self, "_canvas_stack"):
            self._canvas_stack.setCurrentIndex(0 if mode == "2d" else 1)
        # Keep toggle buttons in sync (e.g. when called programmatically)
        if hasattr(self, "_btn_2d") and hasattr(self, "_btn_3d"):
            self._btn_2d.setChecked(mode == "2d")
            self._btn_3d.setChecked(mode == "3d")

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
                    overhang_m=self.overhang,
                )
            )
        except ValueError:
            self._show_toast(tr("unsupportedShape", lang))
            return

        self.last_result = result
        self.canvas.set_overhang(self.overhang)
        self.canvas.set_skeleton(result)
        self.set_results(result)
        self._update_roof_type_badge(fp)
        # G7: update 3D view with walls + roof geometry
        if hasattr(self, "view3d"):
            self.view3d.set_geometry(fp, float(self.pitch), self.wall_height, self.overhang)

    # ------------------------------------------------------------------
    # G5: public helpers for G6
    # ------------------------------------------------------------------

    def get_name(self) -> str:
        """Return the current project name from the Název field."""
        return self.name_edit.text()

    def get_note(self) -> str:
        """Return the current note from the Poznámka field."""
        return self.note_edit.toPlainText()

    def get_location(self):
        """Return the current Location (or None) from the Poloha field."""
        return self.location_field.location

    def get_wall_height(self) -> float:
        """Return the current wall height in metres."""
        return self.wall_height

    def get_overhang(self) -> float:
        """Return the current eave overhang in metres."""
        return self.overhang

    def set_overhang(self, value: float) -> None:
        """Update the overhang value and recompute if a result exists."""
        self.overhang = float(value)
        if hasattr(self, "_overhang_spin"):
            self._overhang_spin.blockSignals(True)
            self._overhang_spin.setValue(self.overhang)
            self._overhang_spin.blockSignals(False)
        if self.last_result is not None and self.canvas.footprint is not None:
            self._on_calculate()

    # ------------------------------------------------------------------
    # G5: roof-type badge helper
    # ------------------------------------------------------------------

    def _update_roof_type_badge(self, fp) -> None:
        """Set the roof-type badge from the footprint + current pitch."""
        from tectora.core.bridge import UnsupportedFootprint, footprint_to_spec
        try:
            spec = footprint_to_spec(fp, float(self.pitch))
            self.roof_type_badge.setText(spec.type.cs_name.capitalize())
        except UnsupportedFootprint:
            self.roof_type_badge.setText("Obecná")

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
            vcount = result.valley_count
            labels["valleys"].setText(
                f"{vcount} ks / {result.valley_length_m:.2f} m"
            )
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

    def _on_save(self) -> None:
        """Save the current project to the library (create or update)."""
        from tectora.application import use_cases as uc
        from tectora.application.project import Project

        lang = self._lang
        fp = self.canvas.footprint
        if fp is None:
            self._show_toast(tr("drawFirst", lang))
            return

        raw_name = self.get_name().strip()
        proj_name = raw_name or tr("untitled", lang)
        project = Project(
            project_name=proj_name,
            footprint=fp,
            global_pitch_deg=float(self.pitch),
            wall_height_m=self.wall_height,
            overhang_m=self.overhang,
        )

        result = None
        try:
            result = uc.calculate_project(project).as_dict()
        except (ValueError, Exception):
            pass

        note = self.get_note()
        location = self.get_location()

        if self._current_entry_id is None:
            eid = uc.save_entry(
                self._window.library, project, proj_name,
                note=note, location=location, result=result,
            )
            self._current_entry_id = eid
        else:
            uc.update_entry_full(
                self._window.library, self._current_entry_id,
                project=project, name=proj_name,
                note=note, location=location, result=result,
            )

        self._show_toast(tr("saved", lang))

    def _export_to(self, path: str) -> None:
        """Write a RoofResult to an Excel file at *path*.

        If no result is available, shows a toast hint and returns.
        Tests call this directly, bypassing the file dialog.
        """
        lang = self._lang
        if self.last_result is None:
            self._show_toast(tr("drawFirst", lang))
            return
        from tectora.io import write_report_xlsx
        write_report_xlsx(self.last_result, path)
        self._show_toast(tr("exportDone", lang))

    def _on_export(self) -> None:
        """Open a Save dialog and call _export_to with the chosen path."""
        from PySide6.QtWidgets import QFileDialog
        path, _ = QFileDialog.getSaveFileName(
            self,
            tr("exportXls", self._lang),
            "",
            "Excel (*.xlsx)",
        )
        if path:
            self._export_to(path)

    # ------------------------------------------------------------------
    # F4: Open / Save portable .tectora file
    # ------------------------------------------------------------------

    def _on_file_open(self) -> None:
        """Open a .tectora file via a file dialog."""
        from PySide6.QtWidgets import QFileDialog
        path, _ = QFileDialog.getOpenFileName(
            self,
            tr("openFile", self._lang),
            "",
            "Tectora (*.tectora *.json)",
        )
        if path:
            self._open_from_file(path)

    def _on_file_save(self) -> None:
        """Save the current drawing to a .tectora file via a file dialog."""
        from PySide6.QtWidgets import QFileDialog
        path, _ = QFileDialog.getSaveFileName(
            self,
            tr("saveFile", self._lang),
            "",
            "Tectora (*.tectora *.json)",
        )
        if path:
            # Ensure .tectora suffix when none given
            import os
            _, ext = os.path.splitext(path)
            if not ext:
                path = path + ".tectora"
            self._save_to_file(path)

    def _save_to_file(self, path: str) -> None:
        """Build a Project from current editor state and write it to *path*."""
        from tectora.application.project import Project
        from tectora.io.project_file import save_project_file

        project = Project(
            project_name=self.get_name() or "Bez názvu",
            footprint=self.canvas.footprint,
            global_pitch_deg=float(self.pitch),
            overhang_m=self.overhang,
            wall_height_m=self.wall_height,
        )
        save_project_file(path, project, note=self.get_note(),
                          location=self.get_location())
        self._show_toast(tr("saveFile", self._lang))

    def _open_from_file(self, path: str) -> None:
        """Load a .tectora file and restore the drawing + parameters."""
        from tectora.io.project_file import load_project_file

        project, note, location = load_project_file(path)
        self.load_project(project, entry_id=None, note=note, location=location)

        # Restore the drawn polygon onto the canvas
        if project.footprint is not None:
            self.canvas._vertices = [(x, y) for (x, y) in project.footprint.vertices]
            self.canvas._edge_types = [e.type.value for e in project.footprint.edges]
            self.canvas._closed = True
            self.canvas.update()

        # Restore individual parameters
        self.set_overhang(project.overhang_m)
        self.wall_height = project.wall_height_m
        self.set_pitch(int(project.global_pitch_deg))

        # Mark as not associated with any library entry
        self._current_entry_id = None

        # Trigger recalculation so the roof shows immediately
        try:
            self._on_calculate()
        except Exception:
            pass

        self._show_toast(tr("openFile", self._lang))

    def set_pitch(self, value: int) -> None:
        """Update the pitch and recompute if a result exists."""
        self.pitch = int(value)
        # Keep spin box in sync without triggering another valueChanged→set_pitch cycle
        self._pitch_spin.blockSignals(True)
        self._pitch_spin.setValue(self.pitch)
        self._pitch_spin.blockSignals(False)
        if self.last_result is not None and self.canvas.footprint is not None:
            self._on_calculate()

    def _pitch_minus(self) -> None:
        """Decrement the pitch by 1, clamped to the minimum of 1°."""
        self.set_pitch(max(1, self.pitch - 1))

    def _pitch_plus(self) -> None:
        """Increment the pitch by 1, clamped to the maximum of 89°."""
        self.set_pitch(min(89, self.pitch + 1))

    def _on_wall_height_changed(self, value: float) -> None:
        """Update wall_height and refresh the 3D view if a result exists."""
        self.wall_height = float(value)
        if self.last_result is not None and hasattr(self, "view3d"):
            fp = self.canvas.footprint
            if fp is not None:
                self.view3d.set_geometry(fp, float(self.pitch), self.wall_height, self.overhang)

    def _on_overhang_changed(self, value: float) -> None:
        """Update overhang and recompute if a result exists."""
        self.set_overhang(float(value))

    def _show_toast(self, text: str) -> None:
        """Show a dark floating toast message that auto-hides after ~2.5 s."""
        toast = QLabel(text, self)
        toast.setStyleSheet(
            f"background: {theme.TEXT};"
            f" color: {theme.CARD};"
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

    def _open_settings(self) -> None:
        """Open the settings dialog (accessible from the Nastavení menu)."""
        from tectora.gui.settings_dialog import SettingsDialog
        current_lang = getattr(self._window, "lang", "cs")
        current_dark = getattr(self._window, "dark", False)
        dlg = SettingsDialog(current_lang=current_lang, current_dark=current_dark, parent=self)
        if dlg.exec():
            lang = dlg.selected_lang()
            dark = dlg.selected_dark()
            if dark != current_dark:
                self._window.set_theme(dark)
            if lang != current_lang:
                self._window.set_language(lang)

    def _on_about(self) -> None:
        """Show the About dialog."""
        from PySide6.QtWidgets import QMessageBox
        QMessageBox.about(self, "Tectora", tr("aboutText", self._lang))
