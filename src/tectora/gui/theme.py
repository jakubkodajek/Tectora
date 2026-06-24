"""Design tokens and global QSS theme for Tectora.

Palette constants (BG, CARD, …) are module-level names that can be
switched between LIGHT and DARK by calling ``set_dark(True/False)``.
Chrome widgets must import the module and reference ``theme.BG`` etc.
at widget-build time so freshly constructed widgets always read the
current palette.
"""
from __future__ import annotations

# ---------------------------------------------------------------------------
# Static palette dicts
# ---------------------------------------------------------------------------

LIGHT: dict[str, str] = {
    "ACCENT":       "#2563eb",
    "ACCENT_HOVER": "#1d4ed8",
    "BG":           "#eef0f2",
    "CARD":         "#ffffff",
    "TEXT":         "#111827",
    "BORDER":       "#e0e2e6",
    "BORDER_SOFT":  "#eceef1",
    "MUTED":        "#6b7280",
    "FAINT":        "#9ca3af",
    "MID":          "#374151",
    "DANGER":       "#dc2626",
}

DARK: dict[str, str] = {
    "ACCENT":       "#2563eb",
    "ACCENT_HOVER": "#3b73ef",
    "BG":           "#1f2430",
    "CARD":         "#262b38",
    "TEXT":         "#e6e8eb",
    "BORDER":       "#3a4150",
    "BORDER_SOFT":  "#2f3542",
    "MUTED":        "#9aa3b2",
    "FAINT":        "#6b7280",
    "MID":          "#c2c8d2",
    "DANGER":       "#ef4444",
}

# Edge/line colours per roof element — constant, never theme-switched
COL: dict[str, str] = {
    "ridge": "#e11d48",
    "hip":   "#9333ea",
    "valley":"#0d9488",
    "eave":  "#2563eb",
    "gable": "#ea580c",
}

# ---------------------------------------------------------------------------
# Module-level mutable palette names (start with LIGHT values)
# ---------------------------------------------------------------------------

ACCENT:       str = LIGHT["ACCENT"]
ACCENT_HOVER: str = LIGHT["ACCENT_HOVER"]
BG:           str = LIGHT["BG"]
CARD:         str = LIGHT["CARD"]
TEXT:         str = LIGHT["TEXT"]
BORDER:       str = LIGHT["BORDER"]
BORDER_SOFT:  str = LIGHT["BORDER_SOFT"]
MUTED:        str = LIGHT["MUTED"]
FAINT:        str = LIGHT["FAINT"]
MID:          str = LIGHT["MID"]
DANGER:       str = LIGHT["DANGER"]

# Internal dark-mode flag
_dark: bool = False


# ---------------------------------------------------------------------------
# GLOBAL_QSS — rebuilt by set_dark()
# ---------------------------------------------------------------------------

def _build_qss() -> str:
    return f"""
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
    background-color: {CARD};
    color: {TEXT};
    border: 1px solid {BORDER};
    border-radius: 8px;
    padding: 7px 16px;
    font-weight: 600;
}}
QPushButton:hover {{
    background-color: {BG};
}}
QPushButton:disabled {{
    color: {FAINT};
}}
QPushButton[primary="true"] {{
    background-color: {ACCENT};
    color: #ffffff;
    border: none;
}}
QPushButton[primary="true"]:hover {{
    background-color: {ACCENT_HOVER};
}}
QPushButton[primary="true"]:pressed {{
    background-color: {ACCENT_HOVER};
}}
QPushButton[primary="true"]:disabled {{
    background-color: {FAINT};
    color: #ffffff;
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


GLOBAL_QSS: str = _build_qss()


# ---------------------------------------------------------------------------
# Public API
# ---------------------------------------------------------------------------

def set_dark(dark: bool) -> None:
    """Switch the palette to DARK or LIGHT and rebuild GLOBAL_QSS."""
    global ACCENT, ACCENT_HOVER, BG, CARD, TEXT, BORDER, BORDER_SOFT
    global MUTED, FAINT, MID, DANGER, GLOBAL_QSS, _dark

    _dark = dark
    palette = DARK if dark else LIGHT
    ACCENT       = palette["ACCENT"]
    ACCENT_HOVER = palette["ACCENT_HOVER"]
    BG           = palette["BG"]
    CARD         = palette["CARD"]
    TEXT         = palette["TEXT"]
    BORDER       = palette["BORDER"]
    BORDER_SOFT  = palette["BORDER_SOFT"]
    MUTED        = palette["MUTED"]
    FAINT        = palette["FAINT"]
    MID          = palette["MID"]
    DANGER       = palette["DANGER"]

    GLOBAL_QSS = _build_qss()


def is_dark() -> bool:
    """Return True if the dark palette is currently active."""
    return _dark


def apply_theme(app) -> None:
    """Apply the current Tectora theme to *app* (a QApplication instance)."""
    from PySide6.QtGui import QFont, QFontDatabase

    families = QFontDatabase.families()
    font_family = "Inter" if "Inter" in families else None

    if font_family:
        font = QFont(font_family, 10)
        app.setFont(font)

    app.setStyleSheet(GLOBAL_QSS)
