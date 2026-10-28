import hashlib
import json
import pathlib
import struct
import sys

SEED = b"primeboost-astra-endpoints/1"
ROOT = pathlib.Path(__file__).resolve().parent.parent
FIELDS = ("site_url", "site_api", "key_api")


def seal(data: bytes) -> bytes:
    out = bytearray()
    for index in range(0, len(data), 32):
        block = hashlib.sha256(SEED + struct.pack("<Q", index // 32)).digest()
        out.extend(byte ^ key for byte, key in zip(data[index:index + 32], block))
    return bytes(out)


def main() -> None:
    source = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "endpoints.local.json"
    values = json.loads(source.read_text(encoding="utf-8"))
    missing = [field for field in FIELDS if not str(values.get(field, "")).strip()]
    if missing:
        sys.exit(f"missing: {', '.join(missing)}")
    payload = json.dumps({field: values[field].rstrip("/") for field in FIELDS}, separators=(",", ":")).encode()
    (ROOT / "endpoints.dat").write_bytes(seal(payload))
    print(f"endpoints.dat written from {source.name}")


if __name__ == "__main__":
    main()
