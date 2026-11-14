"""Verify or reacquire the immutable Writer font catalog with no dependencies."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re
import tempfile
from urllib.parse import unquote, urlsplit
from urllib.request import Request, urlopen


ROOT = Path(__file__).resolve().parent


def validate_entry(entry: dict) -> None:
    """Reject accidental path changes before a maintenance download writes data."""
    name = entry["file"]
    if not name or Path(name).name != name or name in {".", ".."}:
        raise ValueError(f"Invalid local asset filename: {name!r}")
    for key, length in [("commit", 40), ("git_blob_sha1", 40), ("sha256", 64)]:
        if not re.fullmatch(f"[0-9a-f]{{{length}}}", entry[key]):
            raise ValueError(f"Invalid {key} for {name}")
    url = urlsplit(entry["url"])
    expected = "/" + entry["repository"] + "/" + entry["commit"] + "/" + entry["source_path"]
    if (
        url.scheme != "https"
        or url.netloc != "raw.githubusercontent.com"
        or unquote(url.path) != expected
        or url.query
        or url.fragment
    ):
        raise ValueError(f"Asset URL does not match its pinned source: {name}")
    if not 0 < entry["byte_length"] <= 3 * 1024 * 1024:
        raise ValueError(f"Invalid asset byte length: {name}")


def verify_bytes(entry: dict, data: bytes) -> None:
    if len(data) != entry["byte_length"]:
        raise ValueError(f"Unexpected byte length: {entry['file']}")
    if hashlib.sha256(data).hexdigest() != entry["sha256"]:
        raise ValueError(f"SHA-256 mismatch: {entry['file']}")
    blob = b"blob " + str(len(data)).encode("ascii") + b"\0" + data
    if hashlib.sha1(blob).hexdigest() != entry["git_blob_sha1"]:
        raise ValueError(f"Upstream Git blob mismatch: {entry['file']}")


def replace_file(path: Path, data: bytes) -> None:
    """Replace an asset only after complete verification of the downloaded bytes."""
    temporary_path = None
    try:
        with tempfile.NamedTemporaryFile(dir=ROOT, prefix=".font-", delete=False) as temporary:
            temporary_path = Path(temporary.name)
            temporary.write(data)
        temporary_path.replace(path)
    finally:
        if temporary_path is not None:
            temporary_path.unlink(missing_ok=True)


def combined_notice(manifest: dict) -> bytes:
    result = (
        "Writer third-party font notices\nCatalog: "
        + manifest["catalog_id"]
        + "\n\nThese licenses apply to the bundled fonts, not to Office Engine software.\n\n"
    )
    for entry in manifest["entries"]:
        if entry["kind"] == "license":
            result += "=" * 72 + "\n" + entry["file"] + "\nSource: " + entry["url"] + "\n" + "=" * 72 + "\n\n"
            result += (ROOT / entry["file"]).read_text(encoding="utf-8") + "\n\n"
    return result.encode("utf-8")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--download", action="store_true", help="Reacquire pinned files; verification is offline by default.")
    options = parser.parse_args()
    manifest = json.loads((ROOT / "manifest.json").read_text(encoding="utf-8"))
    if manifest["format_version"] != 1:
        raise ValueError("Unsupported font-manifest format")
    for entry in manifest["entries"]:
        validate_entry(entry)
        path = ROOT / entry["file"]
        if options.download:
            request = Request(entry["url"], headers={"User-Agent": "Writer-font-catalog-verification"})
            with urlopen(request, timeout=30) as response:
                data = response.read(entry["byte_length"] + 1)
            verify_bytes(entry, data)
            replace_file(path, data)
        verify_bytes(entry, path.read_bytes())
    notice = combined_notice(manifest)
    if options.download:
        replace_file(ROOT / "NOTICE.txt", notice)
    if (ROOT / "NOTICE.txt").read_bytes() != notice:
        raise ValueError("Combined font notices differ from their upstream source notices")
    for entry in manifest["derived_files"]:
        data = (ROOT / entry["file"]).read_bytes()
        if len(data) != entry["byte_length"] or hashlib.sha256(data).hexdigest() != entry["sha256"]:
            raise ValueError(f"Derived asset checksum mismatch: {entry['file']}")
    fonts = [entry for entry in manifest["entries"] if entry["kind"] == "font"]
    print(f"{manifest['catalog_id']}: verified {len(fonts)} fonts, {sum(entry['byte_length'] for entry in fonts):,} font bytes and all notices.")


if __name__ == "__main__":
    main()
