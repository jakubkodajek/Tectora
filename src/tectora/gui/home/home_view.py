"""HomeView — project library screen (G1)."""
from __future__ import annotations

from typing import TYPE_CHECKING

from PySide6.QtCore import Qt
from PySide6.QtGui import QColor, QFont, QPainter, QPainterPath
from PySide6.QtWidgets import (
    QFrame, QGridLayout, QHBoxLayout, QLabel, QLineEdit,
    QPushButton, QScrollArea, QSizePolicy, QVBoxLayout, QWidget,
)

from tectora.gui.i18n import tr
from tectora.gui import theme
from tectora.gui.home.project_card import ProjectCard

if TYPE_CHECKING:
    pass  # MainWindow imported at runtime to avoid circular imports

_CARD_MIN_W = 248
_CARD_GAP = 20


# ---------------------------------------------------------------------------
# Logo widget
# ---------------------------------------------------------------------------

class _LogoWidget(QWidget):
    """Rounded-square accent background with a house SVG glyph."""

    def __init__(self, parent: QWidget | None = None):
        super().__init__(parent)
        self.setFixedSize(26, 26)

    def paintEvent(self, event):  # noqa: N802
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        path = QPainterPath()
        path.addRoundedRect(0, 0, 26, 26, 6, 6)
        painter.fillPath(path, QColor(theme.ACCENT))
        # House icon: roof line + walls
        pen_color = QColor("#ffffff")
        from PySide6.QtGui import QPen
        pen = QPen(pen_color, 1.7)
        from PySide6.QtCore import Qt as _Qt
        pen.setCapStyle(_Qt.PenCapStyle.RoundCap)
        pen.setJoinStyle(_Qt.PenJoinStyle.RoundJoin)
        painter.setPen(pen)
        painter.setBrush(_Qt.BrushStyle.NoBrush)
        # Roof peak: (13,4) -> (4,11) and (13,4) -> (22,11)
        painter.drawLine(4, 12, 13, 6)
        painter.drawLine(13, 6, 22, 12)
        # Walls: (5,11) -> (5,21) -> (21,21) -> (21,11)
        painter.drawLine(6, 12, 6, 20)
        painter.drawLine(6, 20, 20, 20)
        painter.drawLine(20, 20, 20, 12)
        painter.end()


# ---------------------------------------------------------------------------
# New-project dashed card
# ---------------------------------------------------------------------------

class _NewProjectCard(QWidget):
    """Dashed border card that opens a new editor."""

    def __init__(self, on_click, lang: str, parent: QWidget | None = None):
        super().__init__(parent)
        self._on_click = on_click
        self._lang = lang
        self.setFixedSize(240, 172)
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        self._build_ui()

    def _build_ui(self) -> None:
        layout = QVBoxLayout(self)
        layout.setAlignment(Qt.AlignmentFlag.AlignCenter)
        layout.setSpacing(14)

        # Plus button icon
        icon_btn = QPushButton("+")
        icon_btn.setFixedSize(46, 46)
        icon_btn.setStyleSheet(
            f"QPushButton {{ background:{theme.ACCENT}; color:#fff; border:none;"
            f" border-radius:12px; font-size:22px; font-weight:600; }}"
        )
        icon_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        icon_btn.clicked.connect(self._on_click)
        layout.addWidget(icon_btn, alignment=Qt.AlignmentFlag.AlignCenter)

        # Labels
        title = QLabel(tr("new", self._lang))
        title_font = QFont()
        title_font.setWeight(QFont.Weight.DemiBold)
        title_font.setPointSize(11)
        title.setFont(title_font)
        title.setAlignment(Qt.AlignmentFlag.AlignCenter)
        title.setStyleSheet(f"color:{theme.TEXT}; background:transparent;")
        layout.addWidget(title)

        sub = QLabel(tr("newSub", self._lang))
        sub.setAlignment(Qt.AlignmentFlag.AlignCenter)
        sub.setWordWrap(True)
        sub.setStyleSheet(f"color:{theme.FAINT}; font-size:9pt; background:transparent;")
        layout.addWidget(sub)

    def mousePressEvent(self, event):  # noqa: N802
        self._on_click()

    def paintEvent(self, event):  # noqa: N802
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        from PySide6.QtGui import QPen
        from PySide6.QtCore import Qt as _Qt
        border_col = theme.BORDER if theme.is_dark() else "#c4cad2"
        pen = QPen(QColor(border_col), 1.5, _Qt.PenStyle.DashLine)
        painter.setPen(pen)
        painter.setBrush(QColor(theme.CARD))
        path = QPainterPath()
        path.addRoundedRect(1, 1, self.width() - 2, self.height() - 2, 14, 14)
        painter.drawPath(path)
        painter.end()
        super().paintEvent(event)


# ---------------------------------------------------------------------------
# Responsive grid container
# ---------------------------------------------------------------------------

class _CardGrid(QWidget):
    """Wrapping grid of ProjectCards that reflows on resize."""

    def __init__(self, parent: QWidget | None = None):
        super().__init__(parent)
        self._cards: list[ProjectCard] = []
        self._layout = QGridLayout(self)
        self._layout.setSpacing(_CARD_GAP)
        self._layout.setContentsMargins(0, 0, 0, 0)
        self._cols = 3

    @property
    def cards(self) -> list[ProjectCard]:
        return list(self._cards)

    def set_cards(self, cards: list[ProjectCard]) -> None:
        # Remove all existing widgets from layout
        while self._layout.count():
            item = self._layout.takeAt(0)
            if item.widget():
                item.widget().setParent(None)
        self._cards = list(cards)
        self._reflow()

    def _reflow(self) -> None:
        available_w = self.width() or 900
        cols = max(1, (available_w + _CARD_GAP) // (_CARD_MIN_W + _CARD_GAP))
        self._cols = cols
        for idx, card in enumerate(self._cards):
            row, col = divmod(idx, cols)
            self._layout.addWidget(card, row, col)
        # Fill remaining column slots so last row stretches correctly
        self._layout.setColumnStretch(cols - 1, 1)
        for c in range(cols, 10):
            self._layout.setColumnStretch(c, 0)

    def resizeEvent(self, event):  # noqa: N802
        super().resizeEvent(event)
        available_w = self.width()
        new_cols = max(1, (available_w + _CARD_GAP) // (_CARD_MIN_W + _CARD_GAP))
        if new_cols != self._cols:
            self._reflow()


# ---------------------------------------------------------------------------
# Empty state widget
# ---------------------------------------------------------------------------

class _EmptyState(QWidget):
    def __init__(self, on_new, lang: str, parent: QWidget | None = None):
        super().__init__(parent)
        self._on_new = on_new
        self._lang = lang
        self._build()

    def _build(self) -> None:
        outer = QVBoxLayout(self)
        outer.setContentsMargins(0, 0, 0, 0)

        frame = QFrame()
        frame.setStyleSheet(
            f"QFrame {{ border:1.5px dashed {theme.BORDER}; border-radius:14px;"
            f" background:{theme.CARD}; }}"
        )
        inner = QVBoxLayout(frame)
        inner.setAlignment(Qt.AlignmentFlag.AlignCenter)
        inner.setContentsMargins(24, 64, 24, 64)
        inner.setSpacing(0)

        # Icon placeholder
        icon_box = QWidget()
        icon_box.setFixedSize(64, 64)
        icon_box.setStyleSheet(
            f"background:{theme.BG}; border-radius:16px; border:none;"
        )
        inner.addWidget(icon_box, alignment=Qt.AlignmentFlag.AlignCenter)
        inner.addSpacing(18)

        title = QLabel(tr("emptyTitle", self._lang))
        title_font = QFont()
        title_font.setWeight(QFont.Weight.DemiBold)
        title_font.setPointSize(12)
        title.setFont(title_font)
        title.setAlignment(Qt.AlignmentFlag.AlignCenter)
        title.setStyleSheet(f"color:{theme.TEXT}; border:none; background:transparent;")
        inner.addWidget(title)

        inner.addSpacing(6)

        hint = QLabel(tr("emptyHint", self._lang))
        hint.setAlignment(Qt.AlignmentFlag.AlignCenter)
        hint.setWordWrap(True)
        hint.setMaximumWidth(300)
        hint.setStyleSheet(f"color:{theme.FAINT}; font-size:9pt; border:none; background:transparent;")
        inner.addWidget(hint, alignment=Qt.AlignmentFlag.AlignCenter)

        inner.addSpacing(22)

        btn = QPushButton(tr("new", self._lang))
        btn.setFixedHeight(40)
        btn.setStyleSheet(
            f"QPushButton {{ background:{theme.ACCENT}; color:#fff; border:none;"
            f" border-radius:9px; font-size:10pt; font-weight:600;"
            f" padding:0 20px; }}"
        )
        btn.clicked.connect(self._on_new)
        inner.addWidget(btn, alignment=Qt.AlignmentFlag.AlignCenter)

        outer.addWidget(frame)


# ---------------------------------------------------------------------------
# HomeView
# ---------------------------------------------------------------------------

class HomeView(QWidget):
    """Home screen: top bar + new-project card + scrollable project grid."""

    def __init__(self, window, parent: QWidget | None = None):
        super().__init__(parent)
        self._window = window
        self._lang = getattr(window, "lang", "cs")
        self._search_query = ""
        self.cards: list[ProjectCard] = []
        self._build_ui()
        self.refresh()

    # ------------------------------------------------------------------
    # UI construction
    # ------------------------------------------------------------------

    def _build_ui(self) -> None:
        root = QVBoxLayout(self)
        root.setContentsMargins(0, 0, 0, 0)
        root.setSpacing(0)

        # ── Top bar ──────────────────────────────────────────────────
        topbar = QWidget()
        topbar.setFixedHeight(60)
        topbar.setStyleSheet(
            f"background:{theme.CARD}; border-bottom:1px solid {theme.BORDER};"
        )
        tb_layout = QHBoxLayout(topbar)
        tb_layout.setContentsMargins(24, 0, 24, 0)
        tb_layout.setSpacing(16)

        # Logo + title
        logo = _LogoWidget()
        tb_layout.addWidget(logo)

        brand = QLabel("Tectora")
        brand_font = QFont()
        brand_font.setWeight(QFont.Weight.Bold)
        brand_font.setPointSize(12)
        brand.setFont(brand_font)
        brand.setStyleSheet(f"color:{theme.TEXT}; background:transparent; border:none;")
        tb_layout.addWidget(brand)

        tb_layout.addStretch(1)

        # Search box
        self._search = QLineEdit()
        self._search.setPlaceholderText(tr("search", self._lang))
        self._search.setFixedSize(280, 38)
        self._search.setStyleSheet(
            f"QLineEdit {{ border:1px solid {theme.BORDER}; border-radius:8px;"
            f" background:{theme.CARD}; color:{theme.TEXT}; font-size:10pt;"
            f" padding:0 12px 0 34px; }}"
            f"QLineEdit:focus {{ border-color:{theme.ACCENT}; }}"
        )
        self._search.textChanged.connect(self._on_search)
        tb_layout.addWidget(self._search)

        # Gear button (F2: opens settings dialog — language is set via Settings only)
        self._gear_btn = QPushButton("⚙")
        self._gear_btn.setFixedSize(38, 38)
        self._gear_btn.setStyleSheet(
            f"QPushButton {{ background:{theme.CARD}; color:{theme.MID};"
            f" border:1px solid {theme.BORDER}; border-radius:8px;"
            f" font-size:14pt; }}"
            f"QPushButton:hover {{ background:{theme.BG}; }}"
        )
        self._gear_btn.clicked.connect(self._on_settings)
        tb_layout.addWidget(self._gear_btn)

        root.addWidget(topbar)

        # ── Body (scrollable) ─────────────────────────────────────────
        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setFrameShape(QFrame.Shape.NoFrame)
        scroll.setStyleSheet("background:transparent;")

        body = QWidget()
        body.setStyleSheet(f"background:{theme.BG};")
        body_layout = QVBoxLayout(body)
        body_layout.setContentsMargins(40, 36, 40, 60)
        body_layout.setSpacing(0)

        # New project + welcome row
        hero_row = QHBoxLayout()
        hero_row.setSpacing(20)
        hero_row.setContentsMargins(0, 0, 0, 0)

        new_card = _NewProjectCard(self._on_new, self._lang)
        hero_row.addWidget(new_card, stretch=0)

        welcome = QWidget()
        welcome.setStyleSheet("background:transparent;")
        wl = QVBoxLayout(welcome)
        wl.setContentsMargins(8, 0, 0, 0)
        wl.setAlignment(Qt.AlignmentFlag.AlignVCenter)

        wt = QLabel(tr("welcomeTitle", self._lang))
        wt_font = QFont()
        wt_font.setWeight(QFont.Weight.Bold)
        wt_font.setPointSize(14)
        wt.setFont(wt_font)
        wt.setStyleSheet(f"color:{theme.TEXT}; background:transparent;")
        wl.addWidget(wt)

        ws = QLabel(tr("welcomeSub", self._lang))
        ws.setWordWrap(True)
        ws.setMaximumWidth(440)
        ws.setStyleSheet(f"color:{theme.MUTED}; font-size:10pt; background:transparent;")
        wl.addWidget(ws)

        hero_row.addWidget(welcome, stretch=1)
        body_layout.addLayout(hero_row)
        body_layout.addSpacing(40)

        # Section header: "MOJE ZAKÁZKY" + count
        header_row = QHBoxLayout()
        header_row.setContentsMargins(0, 0, 0, 0)
        header_row.setSpacing(0)

        section_title = QLabel(tr("myProjects", self._lang).upper())
        section_font = QFont()
        section_font.setWeight(QFont.Weight.DemiBold)
        section_font.setPointSize(9)
        section_title.setFont(section_font)
        section_title.setStyleSheet(
            f"color:{theme.MID}; letter-spacing:1px; background:transparent;"
        )
        header_row.addWidget(section_title)
        header_row.addStretch(1)

        self._count_label = QLabel("")
        self._count_label.setStyleSheet(f"color:{theme.FAINT}; font-size:9pt; background:transparent;")
        header_row.addWidget(self._count_label)

        body_layout.addLayout(header_row)
        body_layout.addSpacing(16)

        # Cards grid (or empty state)
        self._grid = _CardGrid()
        self._grid.setStyleSheet("background:transparent;")
        body_layout.addWidget(self._grid)

        # Empty state (hidden by default)
        self._empty_state = _EmptyState(self._on_new, self._lang)
        body_layout.addWidget(self._empty_state)
        self._empty_state.hide()

        body_layout.addStretch(1)

        scroll.setWidget(body)
        root.addWidget(scroll)

    # ------------------------------------------------------------------
    # Public interface
    # ------------------------------------------------------------------

    def refresh(self) -> None:
        """Rebuild the grid from the library (called on init and after delete)."""
        entries = self._window.library.list()
        query = self._search_query.strip().lower()

        if query:
            def _match(e) -> bool:
                if query in e.name.lower():
                    return True
                if e.location and query in e.location.name.lower():
                    return True
                return False
            entries = [e for e in entries if _match(e)]

        new_cards = [ProjectCard(entry, self._window) for entry in entries]
        self.cards = new_cards
        self._grid.set_cards(new_cards)
        self._count_label.setText(str(len(new_cards)))

        if new_cards:
            self._grid.show()
            self._empty_state.hide()
        else:
            self._grid.hide()
            self._empty_state.show()

    # ------------------------------------------------------------------
    # Event handlers
    # ------------------------------------------------------------------

    def _on_new(self) -> None:
        from tectora.application.project import Project
        self._window.open_editor(Project(global_pitch_deg=35.0))

    def _on_search(self, text: str) -> None:
        self._search_query = text
        self.refresh()

    def _on_settings(self) -> None:
        """Open the settings dialog."""
        from tectora.gui.settings_dialog import SettingsDialog
        current_lang = getattr(self._window, "lang", "cs")
        current_mode = getattr(self._window, "mode", "light")
        dlg = SettingsDialog(current_lang=current_lang, current_mode=current_mode, parent=self)
        if dlg.exec():
            lang = dlg.selected_lang()
            mode = dlg.selected_mode()
            if mode != current_mode:
                self._window.set_mode(mode)
            if lang != current_lang:
                self._window.set_language(lang)
