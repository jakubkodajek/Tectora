import json

from tectora.application.project import Project
from tectora.cli.main import main
from tectora.core.footprint import Footprint


def write_project(tmp_path, name="Dům"):
    fp = Footprint.from_lists([(0, 0), (12, 0), (12, 8), (0, 8)],
                              ["eave", "gable", "eave", "gable"])
    p = tmp_path / "proj.json"
    p.write_text(
        json.dumps(Project(project_name=name, footprint=fp,
                           global_pitch_deg=35.0).to_dict()),
        encoding="utf-8",
    )
    return str(p)


def test_library_crud(tmp_path, capsys):
    db = str(tmp_path / "lib.db")
    proj = write_project(tmp_path)

    assert main(["library", "add", proj, "--db", db,
                 "--name", "Dům Novák", "--note", "valba",
                 "--location", "Brno"]) == 0
    eid = json.loads(capsys.readouterr().out)["id"]
    assert eid == 1

    assert main(["library", "list", "--db", db]) == 0
    listed = json.loads(capsys.readouterr().out)
    assert listed[0]["name"] == "Dům Novák"
    assert listed[0]["location"] == "Brno"
    assert listed[0]["area_m2"] is not None

    out = str(tmp_path / "out.json")
    assert main(["library", "get", str(eid), "--db", db, "--output", out]) == 0
    assert json.loads(open(out, encoding="utf-8").read())["project_name"] == "Dům"

    assert main(["library", "update", str(eid), "--db", db,
                 "--name", "Dům Novákovi"]) == 0
    main(["library", "list", "--db", db])
    assert json.loads(capsys.readouterr().out)[0]["name"] == "Dům Novákovi"

    assert main(["library", "delete", str(eid), "--db", db]) == 0
    main(["library", "list", "--db", db])
    assert json.loads(capsys.readouterr().out) == []


def test_geocode_command(monkeypatch, capsys):
    from tectora.io import nominatim_geocoder as ng
    monkeypatch.setattr(
        ng.NominatimGeocoder, "search",
        lambda self, q, limit=5: [ng.Location("Moravský Krumlov", 49.05, 16.31)],
    )
    assert main(["geocode", "Mor"]) == 0
    data = json.loads(capsys.readouterr().out)
    assert data[0]["name"] == "Moravský Krumlov"
    assert data[0]["lat"] == 49.05
