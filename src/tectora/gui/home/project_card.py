"""ProjectCard widget and footprint thumbnail helper."""
from __future__ import annotations

from typing import TYPE_CHECKING

from PySide6.QtCore import QPoint, QRect, Qt
from PySide6.QtGui import (QColor, QFont, QFontMetrics, QPainter, QPainterPath,
                            QPen, QPolygonF)
from PySide6.QtWidgets import (QDialog, QDialogButtonBox, QFormLayout,
                                QHBoxLayout, QInputDialog, QLabel, QLineEdit,
                                QMenu, QMessageBox, QPlainTextEdit, QPushButton,
                                QSizePolicy, QVBoxLayout, QWidget)

from tectora.core.footprint import EdgeType, Footprint
from tectora.gui import theme
from tectora.gui.theme import COL

if TYPE_CHECKING:
    from tectora.application.library import LibraryEntry

# ---------------------------------------------------------------------------
# Pure geometry helper
# ---------------------------------------------------------------------------

def footprint_to_mini_points(
    footprint: Footprint,
    width: int,
    height: int,
    pad: int = 10,
) -> list[tuple[float, float]]:
    """Map footprint vertices into a thumbnail box (pixel space).

    The polygon is scaled uniformly (preserving aspect ratio) and centred
    inside the padded box ``[pad, width-pad] x [pad, height-pad]``.
    Returns a list of (x, y) pixel-space points.
    """
    vertices = footprint.vertices
    if not vertices:
        return []

    xs = [v[0] for v in vertices]
    ys = [v[1] for v in vertices]
    min_x, max_x = min(xs), max(xs)
    min_y, max_y = min(ys), max(ys)

    fp_w = max_x - min_x
    fp_h = max_y - min_y

    box_w = width - 2 * pad
    box_h = height - 2 * pad

    # Scale uniformly; avoid div-by-zero for degenerate footprints
    if fp_w < 1e-9 or fp_h < 1e-9:
        scale = 1.0
    else:
        scale = min(box_w / fp_w, box_h / fp_h)

    scaled_w = fp_w * scale
    scaled_h = fp_h * scale

    # Centre inside padded box
    offset_x = pad + (box_w - scaled_w) / 2
    offset_y = pad + (box_h - scaled_h) / 2

    result = []
    for x, y in vertices:
        px = offset_x + (x - min_x) * scale
        py = offset_y + (y - min_y) * scale
        result.append((px, py))

    return result


# ---------------------------------------------------------------------------
# Thumbnail paint widget
# ---------------------------------------------------------------------------

_PREVIEW_H = 148
_GRID_STEP = 20  # px


class _FootprintPreview(QWidget):
    """Paints a mini footprint polygon on a light grid background."""

    def __init__(self, footprint: Footprint | None, parent: QWidget | None = None):
        super().__init__(parent)
        self._footprint = footprint
        self.setFixedHeight(_PREVIEW_H)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Fixed)
        self.setCursor(Qt.CursorShape.PointingHandCursor)

    def paintEvent(self, event):  # noqa: N802
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)

        w, h = self.width(), self.height()

        # Background — use theme card color slightly lighter
        bg_col = "#fafbfc" if not theme.is_dark() else theme.BG
        painter.fillRect(0, 0, w, h, QColor(bg_col))

        # Grid lines
        grid_col = "#eceef1" if not theme.is_dark() else theme.BORDER_SOFT
        grid_pen = QPen(QColor(grid_col), 1)
        painter.setPen(grid_pen)
        x = _GRID_STEP
        while x < w:
            painter.drawLine(x, 0, x, h)
            x += _GRID_STEP
        y = _GRID_STEP
        while y < h:
            painter.drawLine(0, y, w, y)
            y += _GRID_STEP

        if self._footprint is None or len(self._footprint.vertices) < 3:
            painter.end()
            return

        pts = footprint_to_mini_points(self._footprint, w, h, pad=16)

        # Fill polygon
        poly = QPolygonF()
        for px, py in pts:
            poly.append(QPoint(int(round(px)), int(round(py))))

        fill_color = QColor(theme.ACCENT)
        fill_color.setAlpha(15)
        painter.setBrush(fill_color)
        painter.setPen(Qt.PenStyle.NoPen)
        painter.drawPolygon(poly)

        # Edges coloured by type
        n = len(pts)
        for i, edge in enumerate(self._footprint.edges):
            x1, y1 = pts[i]
            x2, y2 = pts[(i + 1) % n]
            color_hex = COL.get(edge.type.value, theme.ACCENT)
            pen = QPen(QColor(color_hex), 1.6)
            pen.setCapStyle(Qt.PenCapStyle.RoundCap)
            painter.setPen(pen)
            painter.drawLine(
                int(round(x1)), int(round(y1)),
                int(round(x2)), int(round(y2)),
            )

        painter.end()


# ---------------------------------------------------------------------------
# ProjectCard
# ---------------------------------------------------------------------------

class ProjectCard(QWidget):
    """Card widget representing one LibraryEntry."""

    def __init__(self, entry: "LibraryEntry", window, parent: QWidget | None = None):
        super().__init__(parent)
        self._entry = entry
        self._window = window

        self.setMinimumWidth(248)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Fixed)
        self._build_ui()
        self._apply_style()

    # ------------------------------------------------------------------
    # Construction
    # ------------------------------------------------------------------

    def _build_ui(self) -> None:
        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(0)

        # Footprint preview area
        fp = self._entry.project.footprint if self._entry.project else None
        self._preview = _FootprintPreview(fp, self)
        self._preview.mousePressEvent = lambda e: self._open()
        layout.addWidget(self._preview)

        # Separator line under preview
        sep = QWidget()
        sep.setFixedHeight(1)
        sep.setStyleSheet(f"background:{theme.BORDER};")
        layout.addWidget(sep)

        # Info section
        info = QWidget()
        info.setStyleSheet(f"background:{theme.CARD};")
        info_layout = QVBoxLayout(info)
        info_layout.setContentsMargins(14, 12, 14, 14)
        info_layout.setSpacing(3)

        # Name row + menu button
        name_row = QHBoxLayout()
        name_row.setContentsMargins(0, 0, 0, 0)
        name_row.setSpacing(8)

        name_label = QLabel(self._entry.name)
        name_font = QFont()
        name_font.setWeight(QFont.Weight.DemiBold)
        name_font.setPointSize(10)
        name_label.setFont(name_font)
        name_label.setStyleSheet(f"color:{theme.TEXT}; background:transparent;")
        name_label.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Preferred)
        # Ellipsize long names
        name_label.setWordWrap(False)
        fm = QFontMetrics(name_label.font())
        elided = fm.elidedText(self._entry.name, Qt.TextElideMode.ElideRight, 180)
        name_label.setText(elided)
        name_row.addWidget(name_label, stretch=1)

        # ⋮ button
        menu_btn = QPushButton("⋮")
        menu_btn.setFixedSize(28, 28)
        menu_btn.setStyleSheet(
            f"QPushButton {{ background:transparent; border:none; border-radius:7px;"
            f" color:{theme.MUTED}; font-size:16px; }}"
            f"QPushButton:hover {{ background:{theme.BG}; color:{theme.TEXT}; }}"
        )
        menu_btn.clicked.connect(self._show_menu)
        name_row.addWidget(menu_btn, stretch=0)

        info_layout.addLayout(name_row)

        # Meta line: location · date
        meta = self._build_meta()
        meta_label = QLabel(meta)
        meta_label.setStyleSheet(f"color:{theme.FAINT}; font-size:9pt; background:transparent;")
        info_layout.addWidget(meta_label)

        # Area badge
        area = self._build_area()
        if area is not None:
            badge = QLabel(area)
            badge_bg = "#2a3658" if theme.is_dark() else "#f1f5fd"
            badge.setStyleSheet(
                f"background:{badge_bg}; border-radius:6px; padding:4px 9px;"
                f" font-size:9pt; font-weight:600; color:{theme.ACCENT};"
            )
            badge.setSizePolicy(QSizePolicy.Policy.Fixed, QSizePolicy.Policy.Fixed)
            info_layout.addWidget(badge)

        layout.addWidget(info)

    def _build_meta(self) -> str:
        parts = []
        if self._entry.location:
            parts.append(self._entry.location.name)
        date_str = (self._entry.updated_at or "")[:10]  # "YYYY-MM-DD"
        if date_str:
            # format as D.M.YYYY
            try:
                y, m, d = date_str.split("-")
                date_str = f"{int(d)}.{int(m)}.{y}"
            except Exception:
                pass
            parts.append(date_str)
        return " · ".join(parts)

    def _build_area(self) -> str | None:
        if not self._entry.result:
            return None
        area = self._entry.result.get("results", {}).get("total_area_m2")
        if area is None:
            return None
        return f"{area:.1f} m²"

    def _apply_style(self) -> None:
        self.setStyleSheet(
            f"ProjectCard {{"
            f"  background:{theme.CARD};"
            f"  border:1px solid {theme.BORDER};"
            f"  border-radius:14px;"
            f"}}"
            f"ProjectCard:hover {{"
            f"  border-color:{theme.MUTED};"
            f"}}"
        )

    # ------------------------------------------------------------------
    # Actions
    # ------------------------------------------------------------------

    def _open(self) -> None:
        self._window.open_entry(self._entry)

    def _show_menu(self) -> None:
        from tectora.gui.i18n import tr
        lang = getattr(self._window, "lang", "cs")

        menu = QMenu(self)
        menu.setStyleSheet(
            f"QMenu {{ background:{theme.CARD}; border:1px solid {theme.BORDER};"
            f" border-radius:10px; padding:6px; }}"
            f"QMenu::item {{ padding:9px 10px; border-radius:7px;"
            f" font-size:10pt; font-weight:500; }}"
            f"QMenu::item:selected {{ background:{theme.BG}; }}"
        )

        open_action = menu.addAction(tr("open", lang))
        open_action.triggered.connect(self._open)

        menu.addSeparator()

        rename_action = menu.addAction(tr("rename", lang))
        edit_action = menu.addAction(tr("editEntry", lang))
        duplicate_action = menu.addAction(tr("duplicate", lang))

        menu.addSeparator()

        delete_action = menu.addAction(tr("delete", lang))

        # Show menu at button position
        sender = self.sender()
        if sender:
            pos = sender.mapToGlobal(QPoint(0, sender.height()))
        else:
            pos = self.mapToGlobal(QPoint(self.width(), 0))

        chosen = menu.exec(pos)
        if chosen is open_action:
            self._open()
        elif chosen is rename_action:
            self._rename()
        elif chosen is edit_action:
            self._edit_entry()
        elif chosen is duplicate_action:
            self._duplicate()
        elif chosen is delete_action:
            self._confirm_delete()

    def _rename(self) -> None:
        from tectora.application import use_cases as uc
        from tectora.gui.i18n import tr
        lang = getattr(self._window, "lang", "cs")

        new_name, ok = QInputDialog.getText(
            self, tr("rename", lang), tr("name", lang) + ":",
            text=self._entry.name,
        )
        if ok and new_name.strip():
            uc.update_entry(self._window.library, self._entry.id,
                            name=new_name.strip())
            self._window.refresh_home()

    def _edit_entry(self) -> None:
        """Dialog to edit name, note, and location name."""
        from tectora.application import use_cases as uc
        from tectora.core.location import Location
        from tectora.gui.i18n import tr
        lang = getattr(self._window, "lang", "cs")

        dlg = QDialog(self)
        dlg.setWindowTitle(tr("editEntry", lang))
        form = QFormLayout(dlg)

        name_edit = QLineEdit(self._entry.name)
        note_edit = QPlainTextEdit(self._entry.note or "")
        note_edit.setFixedHeight(80)
        loc_name_edit = QLineEdit(
            self._entry.location.name if self._entry.location else ""
        )

        form.addRow(tr("name", lang) + ":", name_edit)
        form.addRow(tr("note", lang) + ":", note_edit)
        form.addRow(tr("location", lang) + ":", loc_name_edit)

        buttons = QDialogButtonBox(
            QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel
        )
        buttons.accepted.connect(dlg.accept)
        buttons.rejected.connect(dlg.reject)
        form.addRow(buttons)

        if dlg.exec() == QDialog.DialogCode.Accepted:
            new_name = name_edit.text().strip() or self._entry.name
            new_note = note_edit.toPlainText()
            loc_text = loc_name_edit.text().strip()
            new_loc = Location(loc_text) if loc_text else None

            uc.update_entry(
                self._window.library, self._entry.id,
                name=new_name, note=new_note, location=new_loc,
            )
            self._window.refresh_home()

    def _duplicate(self) -> None:
        from tectora.application import use_cases as uc
        uc.save_entry(
            self._window.library,
            self._entry.project,
            self._entry.name + " (kopie)",
            note=self._entry.note or "",
            location=self._entry.location,
            result=self._entry.result,
        )
        self._window.refresh_home()

    def _confirm_delete(self) -> None:
        from tectora.application import use_cases as uc
        from tectora.gui.i18n import tr
        lang = getattr(self._window, "lang", "cs")

        msg = QMessageBox(self)
        msg.setWindowTitle(tr("confirmDelete", lang))
        msg.setText(f'Opravdu smazat "{self._entry.name}"?')
        msg.setIcon(QMessageBox.Icon.Question)
        msg.setStandardButtons(
            QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.Cancel
        )
        msg.setDefaultButton(QMessageBox.StandardButton.Cancel)
        if msg.exec() == QMessageBox.StandardButton.Yes:
            uc.delete_entry(self._window.library, self._entry.id)
            self._window.refresh_home()
