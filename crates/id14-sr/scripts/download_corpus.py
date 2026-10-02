"""Fetch the pinned CC0 corpus, verifying Wikimedia's SHA1 before use."""
import hashlib
import json
import pathlib
import random
import secrets
import time
import urllib.error
import urllib.parse
import urllib.request
from datetime import datetime, timezone

ROOT = pathlib.Path(__file__).resolve().parents[1]
MANIFEST = json.loads((ROOT / "corpus/manifest.json").read_text())
DEST = ROOT / "corpus/audio"
DEST.mkdir(parents=True, exist_ok=True)
USER_AGENT = "id14-sr-corpus/0.1 (local research)"
ALPHABET = "0123456789ABCDEFGHJKMNPQRSTVWXYZ"


def ulid():
    value = (int(time.time() * 1000) << 80) | secrets.randbits(80)
    return "".join(ALPHABET[(value >> (5 * shift)) & 31] for shift in range(25, -1, -1))


TRACE_ID = ulid()


def log(msg, **fields):
    print(json.dumps({"ts": datetime.now(timezone.utc).isoformat(), "level": "info", "trace_id": TRACE_ID, "msg": msg, **fields}), flush=True)


def open_url(url):
    for attempt in range(6):
        try:
            return urllib.request.urlopen(urllib.request.Request(url, headers={"User-Agent": USER_AGENT}), timeout=120)
        except urllib.error.HTTPError as exc:
            if exc.code not in (429, 503) or attempt == 5:
                raise
            time.sleep(min(30, 2 ** attempt) + random.random())


for entry in MANIFEST["files"]:
    title = entry["title"]
    query = urllib.parse.urlencode({
        "action": "query", "format": "json", "prop": "imageinfo",
        "iiprop": "url|sha1|extmetadata", "iiextmetadatafilter": "License",
        "iilimit": "1", "titles": title,
    })
    with open_url("https://commons.wikimedia.org/w/api.php?" + query) as reply:
        page = next(iter(json.load(reply)["query"]["pages"].values()))
    info = page["imageinfo"][0]
    license_code = info["extmetadata"]["License"]["value"]
    if license_code != "pd":
        raise RuntimeError(f"{title}: unexpected license {license_code}; inspect file page")
    path = DEST / title.removeprefix("File:").replace(" ", "_")
    if not path.exists():
        part = path.with_suffix(path.suffix + ".part")
        with open_url(info["url"]) as reply, part.open("wb") as output:
            while block := reply.read(1024 * 1024):
                output.write(block)
        part.replace(path)
    sha1 = hashlib.sha1(path.read_bytes()).hexdigest()
    if sha1 != info["sha1"]:
        path.unlink()
        raise RuntimeError(f"{title}: SHA1 mismatch")
    entry["sha1"] = sha1
    entry["url"] = info["descriptionurl"]
    entry["bytes"] = path.stat().st_size
    log("verified", split=entry["split"], file=path.name, bytes=entry["bytes"], sha1=sha1)
    time.sleep(1.0)

(ROOT / "corpus/downloaded.json").write_text(json.dumps(MANIFEST, indent=2) + "\n")
