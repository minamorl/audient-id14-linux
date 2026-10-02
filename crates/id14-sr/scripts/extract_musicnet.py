"""Extract MusicNet WAVs from the verified official tarball into ignored storage."""
import json
import pathlib
import secrets
import shutil
import tarfile
import time
from datetime import datetime, timezone

ROOT = pathlib.Path(__file__).resolve().parents[1]
ARCHIVE = ROOT / "corpus/audio/musicnet.tar.gz"
OUTPUT = ROOT / "corpus/audio/musicnet"
OUTPUT.mkdir(parents=True, exist_ok=True)
ALPHABET = "0123456789ABCDEFGHJKMNPQRSTVWXYZ"
value = (int(time.time() * 1000) << 80) | secrets.randbits(80)
TRACE_ID = "".join(ALPHABET[(value >> (5 * shift)) & 31] for shift in range(25, -1, -1))


def log(msg, **fields):
    print(json.dumps({"ts": datetime.now(timezone.utc).isoformat(), "level": "info", "trace_id": TRACE_ID, "msg": msg, **fields}), flush=True)


count = 0
with tarfile.open(ARCHIVE, "r:gz") as archive:
    for member in archive:
        parts = pathlib.PurePosixPath(member.name).parts
        if not member.isfile() or not member.name.endswith(".wav"):
            continue
        if not any(part in ("train_data", "test_data") for part in parts):
            continue
        if any(part in ("..", "") for part in parts) or pathlib.PurePosixPath(member.name).is_absolute():
            raise RuntimeError("unsafe path in MusicNet archive")
        target = OUTPUT.joinpath(*parts)
        if not target.resolve().is_relative_to(OUTPUT.resolve()):
            raise RuntimeError("MusicNet archive path escapes output")
        target.parent.mkdir(parents=True, exist_ok=True)
        if not target.exists() or target.stat().st_size != member.size:
            source = archive.extractfile(member)
            if source is None:
                raise RuntimeError(f"cannot extract {member.name}")
            temporary = target.with_suffix(target.suffix + ".part")
            with source, temporary.open("wb") as destination:
                shutil.copyfileobj(source, destination, 8 * 1024 * 1024)
            temporary.replace(target)
        count += 1
        if count % 50 == 0:
            log("extracted", recordings=count)
log("complete", recordings=count)
