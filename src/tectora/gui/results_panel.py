"""Panel výsledků pod 3D náhledem."""
from __future__ import annotations

from PySide6.QtCore import Qt
from PySide6.QtGui import QFont
from PySide6.QtWidgets import (
    QFrame,
    QGridLayout,
    QHBoxLayout,
    QHeaderView,
    QLabel,
    QSizePolicy,
    QTableWidget,
    QTableWidgetItem,
    QVBoxLayout,
)

from tectora.core.result import RoofResult
from tectora.gui import style


class ResultsPanel(QFrame):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.setObjectName("panel")
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Preferred)
        self._build_ui()

    def _build_ui(self) -> None:
        outer = QVBoxLayout(self)
        outer.setContentsMargins(16, 14, 16, 14)
        outer.setSpacing(10)

        header = QLabel("Výsledky výpočtu")
        header.setObjectName("sectionHeader")
        outer.addWidget(header)

        # KPI grid (vlajkové metriky)
        kpi_grid = QGridLayout()
        kpi_grid.setSpacing(10)
        self.kpis = {}
        kpi_defs = [
            ("total_area", "Celková plocha", "m²"),
            ("ridge", "Hřeben", "m"),
            ("hips", "Nároží (celkem)", "m"),
            ("eaves", "Okap", "m"),
            ("gable_edges", "Krajové tašky", "m"),
            ("apex", "Výška hřebene", "m"),
        ]
        for i, (key, label, unit) in enumerate(kpi_defs):
            tile = self._build_kpi_tile(label, unit)
            self.kpis[key] = tile
            kpi_grid.addWidget(tile["frame"], i // 3, i % 3)
        outer.addLayout(kpi_grid)

        # Detailní tabulka po rovinách
        self.table = QTableWidget(0, 3)
        self.table.setHorizontalHeaderLabels(["Rovina", "Plocha [m²]", "Sklon [°]"])
        self.table.horizontalHeader().setSectionResizeMode(
            0, QHeaderView.ResizeMode.Stretch
        )
        self.table.horizontalHeader().setSectionResizeMode(
            1, QHeaderView.ResizeMode.ResizeToContents
        )
        self.table.horizontalHeader().setSectionResizeMode(
            2, QHeaderView.ResizeMode.ResizeToContents
        )
        self.table.verticalHeader().setVisible(False)
        self.table.setEditTriggers(QTableWidget.EditTrigger.NoEditTriggers)
        self.table.setAlternatingRowColors(True)
        outer.addWidget(self.table, stretch=1)

        # poznámka
        self.notes_label = QLabel("")
        self.notes_label.setWordWrap(True)
        self.notes_label.setObjectName("subtle")
        outer.addWidget(self.notes_label)

    def _build_kpi_tile(self, label: str, unit: str) -> dict:
        frame = QFrame()
        frame.setObjectName("panel")
        v = QVBoxLayout(frame)
        v.setContentsMargins(12, 8, 12, 8)
        v.setSpacing(2)

        lab = QLabel(label.upper())
        lab.setObjectName("kpiLabel")

        h = QHBoxLayout()
        h.setSpacing(4)
        value = QLabel("—")
        value.setObjectName("kpiValue")
        u = QLabel(unit)
        u.setObjectName("subtle")
        u.setAlignment(Qt.AlignmentFlag.AlignBottom)
        h.addWidget(value)
        h.addWidget(u)
        h.addStretch(1)

        v.addWidget(lab)
        v.addLayout(h)
        return {"frame": frame, "value": value, "unit": u}

    def set_result(self, result: RoofResult | None) -> None:
        if result is None:
            for k in self.kpis.values():
                k["value"].setText("—")
            self.table.setRowCount(0)
            self.notes_label.clear()
            return

        self.kpis["total_area"]["value"].setText(f"{result.total_area_m2:,.2f}".replace(",", " "))
        self.kpis["ridge"]["value"].setText(f"{result.ridge_length_m:,.2f}".replace(",", " "))
        self.kpis["hips"]["value"].setText(f"{result.hip_length_m:,.2f}".replace(",", " "))
        self.kpis["eaves"]["value"].setText(f"{result.eaves_perimeter_m:,.2f}".replace(",", " "))
        self.kpis["gable_edges"]["value"].setText(
            f"{result.gable_edge_total_m:,.2f}".replace(",", " ")
        )
        self.kpis["apex"]["value"].setText(f"{result.apex_height_m:,.2f}".replace(",", " "))

        self.table.setRowCount(len(result.faces))
        for i, face in enumerate(result.faces):
            self.table.setItem(i, 0, QTableWidgetItem(face.name))
            area_item = QTableWidgetItem(f"{face.area_m2:.2f}")
            area_item.setTextAlignment(Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter)
            self.table.setItem(i, 1, area_item)
            slope_item = QTableWidgetItem(f"{face.slope_deg:.1f}")
            slope_item.setTextAlignment(Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter)
            self.table.setItem(i, 2, slope_item)

        # Doplňková řádka s rozpočítáním nároží + štítů
        notes = []
        if result.hip_count > 0:
            notes.append(
                f"Nároží: {result.hip_count} × "
                f"{result.hip_length_m / result.hip_count:.2f} m"
            )
        if result.gable_edge_total_m > 0:
            notes.append(
                f"Krajové tašky: levý štít {result.gable_edge_left_m:.2f} m, "
                f"pravý {result.gable_edge_right_m:.2f} m."
            )
        notes.extend(result.notes)
        self.notes_label.setText("  •  ".join(notes))
