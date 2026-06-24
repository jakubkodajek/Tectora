"""Design tokens and global QSS theme for Tectora."""
from __future__ import annotations

# --- Colour palette ---------------------------------------------------------

ACCENT = "#2563eb"
ACCENT_HOVER = "#1d4ed8"
BG = "#eef0f2"
CARD = "#ffffff"
TEXT = "#111827"
BORDER = "#e0e2e6"
BORDER_SOFT = "#eceef1"
MUTED = "#6b7280"
FAINT = "#9ca3af"
MID = "#374151"
DANGER = "#dc2626"

# Edge/line colours per roof element
COL: dict[str, str] = {
    "ridge": "#e11d48",
    "hip": "#9333ea",
    "valley": "#0d9488",
    "eave": "#2563eb",
    "gable": "#ea580c",
}

# --- Global stylesheet -------------------------------------------------------

GLOBAL_QSS: str = f"""
/* ── App / window backgrounds ─────────────────────────────────────── */
QMainWindow, QDialog, QWidget {{
    background-color: {BG};
    color: {TEXT};
    font-family: "Inter", "Segoe UI", system-ui, sans-serif;
    font-size: 10pt;
}}

/* ── Input fields ──────────────────────────────────────────────────── */
QLineEdit, QPlainTextEdit, QTextEdit {{
    background-color: {CARD};
    color: {TEXT};
    border: 1px solid {BORDER};
    border-radius: 8px;
    padding: 6px 10px;
    selection-background-color: {ACCENT};
    selection-color: {CARD};
}}
QLineEdit:focus, QPlainTextEdit:focus, QTextEdit:focus {{
    border-color: {ACCENT};
}}
QLineEdit:disabled, QPlainTextEdit:disabled {{
    background-color: {BG};
    color: {MUTED};
}}

/* ── Spin / combo boxes ────────────────────────────────────────────── */
QDoubleSpinBox, QSpinBox, QComboBox {{
    background-color: {CARD};
    color: {TEXT};
    border: 1px solid {BORDER};
    border-radius: 8px;
    padding: 5px 8px;
    selection-background-color: {ACCENT};
}}
QDoubleSpinBox:focus, QSpinBox:focus, QComboBox:focus {{
    border-color: {ACCENT};
}}
QComboBox::drop-down {{
    border: none;
    width: 24px;
}}

/* ── Buttons ───────────────────────────────────────────────────────── */
QPushButton {{
    background-color: {ACCENT};
    color: {CARD};
    border: none;
    border-radius: 8px;
    padding: 7px 16px;
    font-weight: 600;
}}
QPushButton:hover {{
    background-color: {ACCENT_HOVER};
}}
QPushButton:pressed {{
    background-color: {ACCENT_HOVER};
}}
QPushButton:disabled {{
    background-color: {FAINT};
    color: {CARD};
}}

/* ── Scroll bars ───────────────────────────────────────────────────── */
QScrollBar:vertical {{
    width: 8px;
    background: transparent;
    margin: 0;
}}
QScrollBar::handle:vertical {{
    background: {BORDER};
    border-radius: 4px;
    min-height: 24px;
}}
QScrollBar::add-line:vertical, QScrollBar::sub-line:vertical {{
    height: 0;
}}

/* ── Menu bar ──────────────────────────────────────────────────────── */
QMenuBar {{
    background-color: {CARD};
    border-bottom: 1px solid {BORDER_SOFT};
}}
QMenuBar::item:selected {{
    background-color: {BG};
    border-radius: 4px;
}}
QMenu {{
    background-color: {CARD};
    border: 1px solid {BORDER};
    border-radius: 6px;
    padding: 4px;
}}
QMenu::item:selected {{
    background-color: {BG};
    border-radius: 4px;
}}

/* ── Status bar ────────────────────────────────────────────────────── */
QStatusBar {{
    background-color: {CARD};
    border-top: 1px solid {BORDER_SOFT};
    color: {MUTED};
    font-size: 9pt;
}}
"""


def apply_theme(app) -> None:
    """Apply the Tectora theme to *app* (a QApplication instance)."""
    from PySide6.QtGui import QFont, QFontDatabase

    # Try to use Inter; fall back gracefully to whatever the system provides.
    families = QFontDatabase.families()
    font_family = "Inter" if "Inter" in families else None

    if font_family:
        font = QFont(font_family, 10)
        app.setFont(font)
    # If Inter is not installed we leave the system default — never crash.

    app.setStyleSheet(GLOBAL_QSS)
