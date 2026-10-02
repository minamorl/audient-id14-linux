"""Verify FMA archives and extract only explicitly CC0/CC BY small tracks.

Official source: https://github.com/mdeff/fma . Each row's raw_tracks.csv
license_url controls selection; tracks.csv's short license title alone does not.
"""
import csv
import hashlib
import json
import pathlib
import shutil
import urllib.parse
import zipfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
DATA = ROOT / "corpus/audio/fma"
ARCHIVES = {
    "fma_metadata.zip": "f0df49ffe5f2a6008d7dc83c6915b31835dfe733",
    "fma_small.zip": "ade154f733639d52e35e32f5593efe5be76c6d70",
}


def sha1(path):
    digest = hashlib.sha1()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(4 * 1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def permitted(url):
    parsed = urllib.parse.urlparse(url.strip().lower())
    if parsed.scheme not in ("http", "https") or parsed.hostname not in (
        "creativecommons.org", "www.creativecommons.org"
    ):
        return False
    return parsed.path.startswith("/licenses/by/") or parsed.path == "/publicdomain/zero/1.0/"


def main():
    for name, expected in ARCHIVES.items():
        actual = sha1(DATA / name)
        if actual != expected:
            raise ValueError(f"{name}: SHA1 {actual} != official {expected}")
        print(f"verified {name} sha1={actual}", flush=True)
    with zipfile.ZipFile(DATA / "fma_metadata.zip") as metadata:
        reader = csv.reader(line.decode("utf-8-sig") for line in metadata.open("fma_metadata/tracks.csv"))
        first, second = next(reader), next(reader)
        next(reader)
        columns = {(a, b): i for i, (a, b) in enumerate(zip(first, second))}
        small = {
            row[0]: {"split": row[columns["set", "split"]], "genre": row[columns["track", "genre_top"]]}
            for row in reader if row[columns["set", "subset"]] == "small"
        }
        raw = csv.DictReader(line.decode("utf-8-sig") for line in metadata.open("fma_metadata/raw_tracks.csv"))
        selected = []
        for row in raw:
            track = small.get(row["track_id"])
            if track is None or not permitted(row["license_url"]):
                continue
            track_id = int(row["track_id"])
            member = f"fma_small/{track_id // 1000:03d}/{track_id:06d}.mp3"
            selected.append({
                "id": track_id,
                "artist_id": row["artist_id"],
                "artist": row["artist_name"],
                "album_id": row["album_id"],
                "title": row["track_title"],
                "genre": track["genre"],
                "split": track["split"],
                "license_title": row["license_title"],
                "license_url": row["license_url"],
                "track_url": row["track_url"],
                "archive_member": member,
            })
    selected.sort(key=lambda item: item["id"])
    artist_splits = {}
    for item in selected:
        artist_splits.setdefault(item["artist_id"], set()).add(item["split"])
    if any(len(splits) > 1 for splits in artist_splits.values()):
        raise ValueError("selected FMA splits share artist IDs")
    with zipfile.ZipFile(DATA / "fma_small.zip") as archive:
        members = set(archive.namelist())
        for item in selected:
            if item["archive_member"] not in members:
                raise ValueError(f"missing MP3 {item['archive_member']}")
            destination = DATA / "selected" / f"{item['id']:06d}.mp3"
            destination.parent.mkdir(parents=True, exist_ok=True)
            if not destination.exists() or destination.stat().st_size != archive.getinfo(item["archive_member"]).file_size:
                with archive.open(item["archive_member"]) as source, destination.open("wb") as target:
                    shutil.copyfileobj(source, target)
            item["sha256"] = hashlib.sha256(destination.read_bytes()).hexdigest()
            item["bytes"] = destination.stat().st_size
    report = {
        "source": "https://github.com/mdeff/fma",
        "archive_sha1": ARCHIVES,
        "selection_rule": "small subset AND raw_tracks.csv license_url on creativecommons.org with path /licenses/by/* or /publicdomain/zero/1.0/; CC BY-NC, BY-ND, BY-SA, ambiguous and missing URLs excluded",
        "artist_disjoint": True,
        "tracks": selected,
    }
    (ROOT / "corpus/fma-selected.json").write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n")
    from collections import Counter
    print("selected", len(selected), "splits", dict(Counter(item["split"] for item in selected)),
          "genres", dict(Counter(item["genre"] for item in selected)), "artists", len(artist_splits), flush=True)


if __name__ == "__main__":
    main()
