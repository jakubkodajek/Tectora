from tectora.gui.i18n import LANGS, tr


def test_langs_cs_sk_en():
    assert set(LANGS) == {"cs", "sk", "en"}


def test_tr_localized_and_fallback():
    assert tr("new", "cs") == "Nová zakázka"
    assert tr("new", "en") == "New project"
    assert tr("nonexistent_key", "cs") == "nonexistent_key"  # fallback = key
