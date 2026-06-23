"""Levý panel — vstupní formulář."""
from __future__ import annotations

from PySide6.QtCore import Qt, Signal
from PySide6.QtWidgets import (
    QComboBox,
    QDoubleSpinBox,
    QFormLayout,
    QFrame,
    QLabel,
    QLineEdit,
    QPushButton,
    QSizePolicy,
    QVBoxLayout,
)

from tectora.core.spec import RoofSpec, RoofType


class FormPanel(QFrame):
    """Form s parametry střechy + tlačítko Provést výpočet."""

    calculate_requested = Signal()
    type_changed = Signal()

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setObjectName("panel")
        self.setSizePolicy(QSizePolicy.Policy.Fixed, QSizePolicy.Policy.Expanding)
        self.setMinimumWidth(340)
        self.setMaximumWidth(380)
        self._build_ui()

    def _build_ui(self) -> None:
        outer = QVBoxLayout(self)
        outer.setContentsMargins(20, 20, 20, 20)
        outer.setSpacing(14)

        title = QLabel("Parametry střechy")
        title.setObjectName("sectionHeader")
        outer.addWidget(title)

        subtitle = QLabel("Zadej rozměry, sklon a typ. Vše v metrech a stupních.")
        subtitle.setObjectName("subtle")
        subtitle.setWordWrap(True)
        outer.addWidget(subtitle)

        form = QFormLayout()
        form.setSpacing(10)
        form.setLabelAlignment(Qt.AlignmentFlag.AlignRight)

        self.name_edit = QLineEdit()
        self.name_edit.setPlaceholderText("nepovinné — např. „Domek u potoka“")
        form.addRow("Název projektu:", self.name_edit)

        self.type_combo = QComboBox()
        for t in RoofType:
            self.type_combo.addItem(f"{t.cs_name} ({t.value})", t)
        self.type_combo.setCurrentIndex(1)  # sedlová default
        self.type_combo.currentIndexChanged.connect(self._on_type_changed)
        form.addRow("Typ:", self.type_combo)

        self.length_spin = _spin(0.5, 200.0, 0.1, 12.0, suffix=" m")
        form.addRow("Délka (L):", self.length_spin)

        self.width_spin = _spin(0.5, 200.0, 0.1, 8.0, suffix=" m")
        form.addRow("Šířka (W):", self.width_spin)

        self.pitch_spin = _spin(1.0, 89.0, 0.5, 35.0, suffix=" °")
        form.addRow("Sklon:", self.pitch_spin)

        self.pitch_back_spin = _spin(1.0, 89.0, 0.5, 35.0, suffix=" °")
        self.pitch_back_check = QComboBox()
        self.pitch_back_check.addItems(["symetrický", "vlastní"])
        self.pitch_back_check.currentIndexChanged.connect(self._on_pitch_back_toggle)
        form.addRow("Zadní sklon:", self.pitch_back_check)
        self.pitch_back_label = QLabel("hodnota zadního sklonu:")
        self.pitch_back_label.setObjectName("subtle")
        form.addRow(self.pitch_back_label, self.pitch_back_spin)
        self.pitch_back_spin.setEnabled(False)
        self.pitch_back_label.setEnabled(False)

        self.ratio_spin = _spin(0.0, 1.0, 0.05, 0.5, suffix="")
        form.addRow("Poměr valby:", self.ratio_spin)

        self.overhang_spin = _spin(0.0, 2.0, 0.05, 0.0, suffix=" m")
        form.addRow("Přesah okapu:", self.overhang_spin)

        outer.addLayout(form)
        outer.addStretch(1)

        # Tlačítka
        self.calc_button = QPushButton("✓  Provést výpočet")
        self.calc_button.setObjectName("primary")
        self.calc_button.setMinimumHeight(40)
        self.calc_button.clicked.connect(self.calculate_requested)
        outer.addWidget(self.calc_button)

        self.reset_button = QPushButton("Reset na výchozí")
        self.reset_button.setObjectName("secondary")
        self.reset_button.clicked.connect(self._on_reset)
        outer.addWidget(self.reset_button)

        self._update_field_visibility()

    def _on_type_changed(self) -> None:
        self._update_field_visibility()
        self.type_changed.emit()

    def _on_pitch_back_toggle(self) -> None:
        enabled = self.pitch_back_check.currentText() == "vlastní"
        self.pitch_back_spin.setEnabled(enabled)
        self.pitch_back_label.setEnabled(enabled)

    def _on_reset(self) -> None:
        self.name_edit.clear()
        self.type_combo.setCurrentIndex(1)
        self.length_spin.setValue(12.0)
        self.width_spin.setValue(8.0)
        self.pitch_spin.setValue(35.0)
        self.pitch_back_check.setCurrentIndex(0)
        self.ratio_spin.setValue(0.5)
        self.overhang_spin.setValue(0.0)

    def _update_field_visibility(self) -> None:
        t = self.current_type()
        self.ratio_spin.setEnabled(t == RoofType.HALF_HIP)
        self.pitch_back_check.setEnabled(t == RoofType.GABLE)
        if t != RoofType.GABLE:
            self.pitch_back_check.setCurrentIndex(0)
        # Stanová → uzamknout width = length (jen jednou připojíme)
        if not hasattr(self, "_length_sync_connected"):
            self.length_spin.valueChanged.connect(self._sync_width)
            self._length_sync_connected = True
        self._pyramid_mode = (t == RoofType.PYRAMID)
        if self._pyramid_mode:
            self.width_spin.setValue(self.length_spin.value())
        self.width_spin.setEnabled(not self._pyramid_mode)

    def _sync_width(self, value):
        if getattr(self, "_pyramid_mode", False):
            self.width_spin.setValue(value)

    # ---------- API ----------
    def current_type(self) -> RoofType:
        return self.type_combo.currentData()

    def build_spec(self) -> RoofSpec:
        pitch_back = (
            self.pitch_back_spin.value()
            if self.pitch_back_check.currentText() == "vlastní"
            else None
        )
        return RoofSpec(
            type=self.current_type(),
            length=self.length_spin.value(),
            width=self.width_spin.value(),
            pitch=self.pitch_spin.value(),
            pitch_back=pitch_back,
            half_hip_ratio=self.ratio_spin.value(),
            eave_overhang=self.overhang_spin.value(),
            name=self.name_edit.text().strip(),
        )

    def load_spec(self, spec: RoofSpec) -> None:
        self.name_edit.setText(spec.name)
        # set type
        for i in range(self.type_combo.count()):
            if self.type_combo.itemData(i) == spec.type:
                self.type_combo.setCurrentIndex(i)
                break
        self.length_spin.setValue(spec.length)
        self.width_spin.setValue(spec.width)
        self.pitch_spin.setValue(spec.pitch)
        if spec.pitch_back is not None:
            self.pitch_back_check.setCurrentIndex(1)
            self.pitch_back_spin.setValue(spec.pitch_back)
        else:
            self.pitch_back_check.setCurrentIndex(0)
        self.ratio_spin.setValue(spec.half_hip_ratio)
        self.overhang_spin.setValue(spec.eave_overhang)


def _spin(minv: float, maxv: float, step: float, default: float, suffix: str = "") -> QDoubleSpinBox:
    sb = QDoubleSpinBox()
    sb.setRange(minv, maxv)
    sb.setSingleStep(step)
    sb.setValue(default)
    sb.setDecimals(2)
    sb.setSuffix(suffix)
    sb.setAlignment(Qt.AlignmentFlag.AlignRight)
    return sb
