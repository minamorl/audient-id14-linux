import json

from remix_train.data import discover_moises, discover_slakh


def test_official_moises_layout(tmp_path):
    track = tmp_path / "provider" / "uuid"
    stems = []
    for index, name in enumerate(("vocals", "drums", "bass", "guitar")):
        directory = track / name
        directory.mkdir(parents=True)
        (directory / f"s{index}.wav").touch()
        stems.append(
            {
                "stemName": name,
                "tracks": [{"id": f"s{index}", "extension": "wav"}],
            }
        )
    (track / "data.json").write_text(json.dumps({"stems": stems}))
    found = discover_moises(tmp_path)
    assert len(found) == 1
    assert [len(group) for group in found[0].stems] == [1, 1, 1, 1]


def test_official_slakh_layout(tmp_path):
    track = tmp_path / "train" / "Track00001"
    stems = track / "stems"
    stems.mkdir(parents=True)
    for name in ("S00", "S01", "S02"):
        (stems / f"{name}.flac").touch()
    (track / "mix.flac").touch()
    (track / "metadata.yaml").write_text(
        """stems:
  S00:
    inst_class: Drums
  S01:
    inst_class: Bass
  S02:
    inst_class: Piano
"""
    )
    found = discover_slakh(tmp_path)
    assert len(found) == 1
    assert found[0].split == "train"
    assert [len(group) for group in found[0].stems] == [0, 1, 1, 1]

