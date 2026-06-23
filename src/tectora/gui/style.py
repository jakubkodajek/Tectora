"""Stylesheet and colours for the GUI — modern light palette with blue accents."""

# Primary palette (light with blue accents)
PRIMARY = "#1F4E78"
PRIMARY_LIGHT = "#2E75B6"
ACCENT = "#F2A900"
BG = "#F5F7FA"
PANEL = "#FFFFFF"
TEXT = "#1A1A1A"
TEXT_MUTED = "#6C757D"
BORDER = "#DEE2E6"
SUCCESS = "#2E7D32"

# Colours for 3D rendering (flat shading with lighting)
ROOF_BASE = (235, 130, 80)        # brick orange
ROOF_HIGHLIGHT = (250, 180, 140)
ROOF_SHADOW = (170, 80, 40)
WALL = (245, 245, 235)
WALL_SHADOW = (200, 200, 190)
GROUND = (220, 220, 215)
EDGE = (40, 40, 40)


STYLESHEET = f"""
QMainWindow, QWidget#centralWidget {{
    background-color: {BG};
    color: {TEXT};
    font-family: "Segoe UI", "Inter", sans-serif;
    font-size: 10pt;
}}

QFrame#panel {{
    background-color: {PANEL};
    border: 1px solid {BORDER};
    border-radius: 8px;
}}

QLabel#sectionHeader {{
    color: {PRIMARY};
    font-size: 12pt;
    font-weight: 600;
    padding: 4px 0 8px 0;
}}

QLabel#projectTitle {{
    color: {PRIMARY};
    font-size: 18pt;
    font-weight: 700;
}}

QLabel#subtle {{
    color: {TEXT_MUTED};
    font-size: 9pt;
}}

QLabel#kpiLabel {{
    color: {TEXT_MUTED};
    font-size: 9pt;
}}

QLabel#kpiValue {{
    color: {PRIMARY};
    font-size: 14pt;
    font-weight: 700;
}}

QPushButton#primary {{
    background-color: {PRIMARY};
    color: white;
    border: none;
    padding: 10px 20px;
    border-radius: 6px;
    font-weight: 600;
    font-size: 11pt;
}}
QPushButton#primary:hover {{ background-color: {PRIMARY_LIGHT}; }}
QPushButton#primary:pressed {{ background-color: {PRIMARY}; }}

QPushButton#secondary {{
    background-color: {PANEL};
    color: {PRIMARY};
    border: 1px solid {PRIMARY};
    padding: 8px 16px;
    border-radius: 6px;
    font-weight: 500;
}}
QPushButton#secondary:hover {{ background-color: #EAF2FA; }}

QLineEdit, QDoubleSpinBox, QSpinBox, QComboBox {{
    background-color: white;
    border: 1px solid {BORDER};
    border-radius: 4px;
    padding: 6px 8px;
    selection-background-color: {PRIMARY_LIGHT};
}}
QLineEdit:focus, QDoubleSpinBox:focus, QSpinBox:focus, QComboBox:focus {{
    border-color: {PRIMARY};
}}

QComboBox::drop-down {{ border: none; width: 24px; }}

QTableWidget {{
    background-color: {PANEL};
    border: 1px solid {BORDER};
    border-radius: 4px;
    gridline-color: {BORDER};
}}
QTableWidget::item {{ padding: 6px; }}
QHeaderView::section {{
    background-color: {PRIMARY};
    color: white;
    padding: 6px;
    border: none;
    font-weight: 600;
}}

QStatusBar {{ background-color: {PRIMARY}; color: white; }}
"""
