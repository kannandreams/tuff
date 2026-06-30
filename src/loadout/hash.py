from __future__ import annotations

from hashlib import sha256
from pathlib import Path


def hash_bytes(content: bytes) -> str:
    return sha256(content).hexdigest()


def hash_file(path: Path) -> str:
    return hash_bytes(path.read_bytes())
