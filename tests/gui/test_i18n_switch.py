import os
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

from tectora.gui.i18n import CATALOG, LANGS, tr


def test_catalog_keys_identical_across_languages():
    keysets = {lang: set(CATALOG[lang]) for lang in LANGS}
    base = keysets["cs"]
    for lang in LANGS:
        assert keysets[lang] == base, (lang, base ^ keysets[lang])


def test_tr_each_lang_nonempty():
    for key in CATALOG["cs"]:
        for lang in LANGS:
            assert tr(key, lang).strip() != ""


def _win(tmp_path):
    from PySide6.QtWidgets import QApplication
    from tectora.gui.app_window import MainWindow
    from tectora.io.sqlite_library import SqliteProjectLibrary
    app = QApplication.instance() or QApplication([])
    lib = SqliteProjectLibrary(str(tmp_path / "lib.db"))
    return app, MainWindow(library=lib)


def test_set_language_updates_lang(tmp_path):
    app, w = _win(tmp_path)
    w.set_language("en")
    app.processEvents()
    assert w.lang == "en"
    # home rebuilt and still functional
    assert w._stack.widget(0) is not None


def test_set_language_preserves_editor_project(tmp_path):
    from tectora.application.project import Project
    app, w = _win(tmp_path)
    w.open_editor(Project(project_name="Dům", global_pitch_deg=35.0))
    w.set_language("sk")
    app.processEvents()
    assert w.lang == "sk"
    assert w._stack.currentIndex() == 1   # still on the editor
