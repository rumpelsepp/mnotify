"""Write a solid-colour RGB PNG: make_png.py WIDTH HEIGHT PATH (stdlib only)."""

import struct
import sys
import zlib


def chunk(kind: bytes, data: bytes) -> bytes:
    body = kind + data
    return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))


width, height, path = int(sys.argv[1]), int(sys.argv[2]), sys.argv[3]
row = b"\x00" + b"\x20\x80\xc0" * width
png = (
    b"\x89PNG\r\n\x1a\n"
    + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
    + chunk(b"IDAT", zlib.compress(row * height))
    + chunk(b"IEND", b"")
)
with open(path, "wb") as f:
    f.write(png)
