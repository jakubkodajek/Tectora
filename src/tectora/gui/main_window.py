"""Main window of the Tectora application."""
from __future__ import annotations

from pathlib import Path

from PySide6.QtCore import Qt
from PySide6.QtGui import QAction, QKeySequence
from PySide6.QtWidgets import (
    QFileDialog,
    QHBoxLayout,
    QMainWindow,
    QMessageBox,
    QStatusBar,
    QVBoxLayout,
    QWidget,
)

from tectora.core import calculate
from tectora.core.result import RoofResult
from tectora.gui import style
from tectora.gui.form_panel import FormPanel
from tectora.gui.results_panel import ResultsPanel
from tectora.gui.view_3d import Roof3DView
from tectora.io import (
    read_spec_json,
    read_spec_xlsx,
    write_report_xlsx,
    write_result_json,
    write_template_xlsx,
)


class MainWindow(QMainWindow):
    def __init__(self):
        super().__init__()
        self.setWindowTitle("Tectora — kalkulátor střech")
        self.resize(1280, 800)
        self.setMinimumSize(1024, 640)

        self._result: RoofResult | None = None
        self._build_ui()
        self._build_menu()

        self.setStatusBar(QStatusBar())
        self.statusBar().showMessage("Připraveno. Zadej parametry a klikni „Provést výpočet“.")

    def _build_ui(self) -> None:
        central = QWidget()
        central.setObjectName("centralWidget")
        self.setCentralWidget(central)

        layout = QHBoxLayout(central)
        layout.setContentsMargins(16, 16, 16, 16)
        layout.setSpacing(16)

        self.form_panel = FormPanel()
        self.form_panel.calculate_requested.connect(self._on_calculate)
        self.form_panel.type_changed.connect(self._on_type_changed)

        right = QWidget()
        right_layout = QVBoxLayout(right)
        right_layout.setContentsMargins(0, 0, 0, 0)
        right_layout.setSpacing(12)

        self.view_3d = Roof3DView()
        right_layout.addWidget(self.view_3d, stretch=3)

        self.results_panel = ResultsPanel()
        right_layout.addWidget(self.results_panel, stretch=2)

        layout.addWidget(self.form_panel)
        layout.addWidget(right, stretch=1)

    def _build_menu(self) -> None:
        menubar = self.menuBar()

        file_menu = menubar.addMenu("&Soubor")

        open_xlsx = QAction("Načíst vstup (Excel)…", self)
        open_xlsx.setShortcut(QKeySequence("Ctrl+O"))
        open_xlsx.triggered.connect(self._on_open_xlsx)
        file_menu.addAction(open_xlsx)

        open_json = QAction("Načíst vstup (JSON)…", self)
        open_json.triggered.connect(self._on_open_json)
        file_menu.addAction(open_json)

        file_menu.addSeparator()

        export_xlsx = QAction("Exportovat výkaz (Excel)…", self)
        export_xlsx.setShortcut(QKeySequence("Ctrl+S"))
        export_xlsx.triggered.connect(self._on_export_xlsx)
        file_menu.addAction(export_xlsx)

        export_json = QAction("Exportovat výsledek (JSON)…", self)
        export_json.triggered.connect(self._on_export_json)
        file_menu.addAction(export_json)

        template_action = QAction("Vytvořit prázdnou Excel šablonu…", self)
        template_action.triggered.connect(self._on_create_template)
        file_menu.addAction(template_action)

        file_menu.addSeparator()

        quit_action = QAction("Konec", self)
        quit_action.setShortcut(QKeySequence("Ctrl+Q"))
        quit_action.triggered.connect(self.close)
        file_menu.addAction(quit_action)

        view_menu = menubar.addMenu("&Pohled")
        reset_view = QAction("Reset 3D pohledu", self)
        reset_view.setShortcut(QKeySequence("Ctrl+R"))
        reset_view.triggered.connect(self.view_3d.reset_view)
        view_menu.addAction(reset_view)

        help_menu = menubar.addMenu("&Nápověda")
        about = QAction("O aplikaci…", self)
        about.triggered.connect(self._on_about)
        help_menu.addAction(about)

    # ---------- actions ----------
    def _on_calculate(self) -> None:
        try:
            spec = self.form_panel.build_spec()
            result = calculate(spec)
        except ValueError as exc:
            QMessageBox.warning(self, "Chybný vstup", str(exc))
            self.statusBar().showMessage(f"Chyba: {exc}", 5000)
            return

        self._result = result
        self.view_3d.set_result(result)
        self.results_panel.set_result(result)
        self.statusBar().showMessage(
            f"Výpočet hotový — plocha {result.total_area_m2:.2f} m², "
            f"hřeben {result.ridge_length_m:.2f} m, "
            f"nároží {result.hip_length_m:.2f} m.",
            10000,
        )

    def _on_type_changed(self) -> None:
        # On type change, hide old results so the user knows a recalculation is needed
        self.statusBar().showMessage("Typ střechy změněn — klikni „Provést výpočet“.")

    def _on_open_xlsx(self) -> None:
        path, _ = QFileDialog.getOpenFileName(
            self, "Načíst vstup z Excelu", "", "Excel soubory (*.xlsx *.xlsm)"
        )
        if not path:
            return
        try:
            spec = read_spec_xlsx(Path(path))
            self.form_panel.load_spec(spec)
            self.statusBar().showMessage(f"Načteno: {path}", 5000)
        except Exception as exc:  # noqa: BLE001
            QMessageBox.critical(self, "Chyba načítání", str(exc))

    def _on_open_json(self) -> None:
        path, _ = QFileDialog.getOpenFileName(
            self, "Načíst vstup z JSON", "", "JSON soubory (*.json)"
        )
        if not path:
            return
        try:
            spec = read_spec_json(Path(path))
            self.form_panel.load_spec(spec)
            self.statusBar().showMessage(f"Načteno: {path}", 5000)
        except Exception as exc:  # noqa: BLE001
            QMessageBox.critical(self, "Chyba načítání", str(exc))

    def _on_export_xlsx(self) -> None:
        if self._result is None:
            QMessageBox.information(self, "Nic k exportu", "Nejdříve proveď výpočet.")
            return
        suggested = (self._result.spec.name or "vykaz") + ".xlsx"
        path, _ = QFileDialog.getSaveFileName(
            self, "Uložit výkaz", suggested, "Excel soubory (*.xlsx)"
        )
        if not path:
            return
        try:
            write_report_xlsx(self._result, Path(path))
            self.statusBar().showMessage(f"Uloženo: {path}", 5000)
        except Exception as exc:  # noqa: BLE001
            QMessageBox.critical(self, "Chyba exportu", str(exc))

    def _on_export_json(self) -> None:
        if self._result is None:
            QMessageBox.information(self, "Nic k exportu", "Nejdříve proveď výpočet.")
            return
        suggested = (self._result.spec.name or "vysledek") + ".json"
        path, _ = QFileDialog.getSaveFileName(
            self, "Uložit JSON", suggested, "JSON soubory (*.json)"
        )
        if not path:
            return
        try:
            write_result_json(self._result, Path(path))
            self.statusBar().showMessage(f"Uloženo: {path}", 5000)
        except Exception as exc:  # noqa: BLE001
            QMessageBox.critical(self, "Chyba exportu", str(exc))

    def _on_create_template(self) -> None:
        path, _ = QFileDialog.getSaveFileName(
            self, "Vytvořit Excel šablonu", "tectora-sablona.xlsx", "Excel soubory (*.xlsx)"
        )
        if not path:
            return
        try:
            write_template_xlsx(Path(path))
            self.statusBar().showMessage(f"Šablona vytvořena: {path}", 5000)
        except Exception as exc:  # noqa: BLE001
            QMessageBox.critical(self, "Chyba", str(exc))

    def _on_about(self) -> None:
        QMessageBox.about(
            self,
            "O aplikaci",
            "<b>Tectora</b> v0.1.0<br>"
            "Kalkulátor střech — desktopová aplikace a CLI.<br><br>"
            "Fáze 1: obdélníkový půdorys, 5 typů střech.<br>"
            "Fáze 2: CLI režim pro AI (Claude/Gemini).<br>"
            "Fáze 3: obecné polygonální půdorysy.",
        )
