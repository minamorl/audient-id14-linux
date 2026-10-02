"""Resume and verify the official MusicNet archive from Zenodo record 5120004."""
import hashlib
import json
import pathlib
import random
import secrets
import time
import urllib.error
import urllib.request
from datetime import datetime, timezone

ROOT = pathlib.Path(__file__).resolve().parents[1]
DEST = ROOT / "corpus/audio"
DEST.mkdir(parents=True, exist_ok=True)
API = "https://zenodo.org/api/records/5120004"
USER_AGENT = "id14-sr-musicnet/0.2 (local research)"
ALPHABET = "0123456789ABCDEFGHJKMNPQRSTVWXYZ"


def ulid():
    value = (int(time.time() * 1000) << 80) | secrets.randbits(80)
    return "".join(ALPHABET[(value >> (5 * shift)) & 31] for shift in range(25, -1, -1))


TRACE_ID = ulid()


def log(msg, **fields):
    print(json.dumps({"ts": datetime.now(timezone.utc).isoformat(), "level": "info", "trace_id": TRACE_ID, "msg": msg, **fields}), flush=True)


def request(url, start=None):
    headers = {"User-Agent": USER_AGENT}
    if start is not None:
        headers["Range"] = f"bytes={start}-"
    return urllib.request.Request(url, headers=headers)


def open_retry(url, start=None):
    for attempt in range(6):
        try:
            return urllib.request.urlopen(request(url, start), timeout=120)
        except (urllib.error.HTTPError, urllib.error.URLError, TimeoutError) as exc:
            code = getattr(exc, "code", None)
            if attempt == 5 or (code is not None and code not in (429, 500, 502, 503, 504)):
                raise
            time.sleep(min(30, 2 ** attempt) + random.random())


with open_retry(API) as response:
    record = json.load(response)
license_id = record["metadata"]["license"]["id"]
if license_id != "cc-by-4.0":
    raise RuntimeError(f"unexpected MusicNet license {license_id}")
files = {item["key"]: item for item in record["files"]}

for name in ("musicnet_metadata.csv", "musicnet.tar.gz"):
    info = files[name]
    size = info["size"]
    path = DEST / name
    position = path.stat().st_size if path.exists() else 0
    if position > size:
        raise RuntimeError(f"{name}: local file exceeds official size")
    stalled = 0
    while position < size:
        before = position
        with open_retry(info["links"]["self"], start=position if position else None) as response:
            if position and response.status != 206:
                raise RuntimeError(f"{name}: server did not honor range request")
            if position and not response.headers.get("Content-Range", "").startswith(f"bytes {position}-"):
                raise RuntimeError(f"{name}: incorrect content range")
            with path.open("ab") as output:
                while True:
                    try:
                        block = response.read(8 * 1024 * 1024)
                    except (TimeoutError, OSError):
                        break
                    if not block:
                        break
                    output.write(block)
                    position += len(block)
                    if position // (512 * 1024 * 1024) != (position - len(block)) // (512 * 1024 * 1024):
                        log("download_progress", file=name, bytes=position, expected_bytes=size)
        if position < size:
            log("resume", file=name, bytes=position, expected_bytes=size)
            stalled = stalled + 1 if position == before else 0
            if stalled > 5:
                raise RuntimeError(f"{name}: no download progress after 5 retries")
            time.sleep(min(30, 2 ** stalled) + random.random())
    if position != size:
        raise RuntimeError(f"{name}: size mismatch")
    digest = hashlib.md5()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(8 * 1024 * 1024), b""):
            digest.update(block)
    actual = "md5:" + digest.hexdigest()
    if actual != info["checksum"]:
        raise RuntimeError(f"{name}: checksum mismatch: {actual} != {info['checksum']}")
    log("verified", file=name, bytes=size, checksum=actual)

(ROOT / "corpus/musicnet-source.json").write_text(json.dumps({
    "record": API,
    "license": license_id,
    "files": [{"key": files[name]["key"], "size": files[name]["size"], "checksum": files[name]["checksum"]} for name in ("musicnet_metadata.csv", "musicnet.tar.gz")],
}, indent=2) + "\n")
