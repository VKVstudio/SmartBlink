"""Generate SmartBlink icon assets (icon.ico with 16/24/32/48 px PNG-compressed frames).

Design: dark rounded square, white keyboard keycap, orange "shift" arrow glyph.
Pure stdlib (struct + zlib via a minimal deflate using stored blocks is complex;
instead use zlib.compress with raw zlib stream wrapped into PNG IDAT).
"""
import math
import os
import struct
import zlib

OUT_DIR = os.path.join(os.path.dirname(__file__), "..", "assets")


def make_image(size: int) -> bytes:
    """Render RGBA pixels for one icon frame, return raw rows."""
    px = bytearray(size * size * 4)
    r_out = size * 0.22  # outer corner radius
    pad = max(1, size // 12)  # keycap inset

    def in_round_rect(x, y, x0, y0, x1, y1, rad):
        if x < x0 or x >= x1 or y < y0 or y >= y1:
            return False
        cx = min(max(x, x0 + rad), x1 - rad)
        cy = min(max(y, y0 + rad), y1 - rad)
        return (x - cx) ** 2 + (y - cy) ** 2 <= rad * rad

    # Shift-arrow glyph: chevron head + stem, inside keycap.
    def in_arrow(x, y, kx0, ky0, kx1, ky1):
        w = kx1 - kx0
        h = ky1 - ky0
        cx = (kx0 + kx1) / 2.0
        # stem: vertical bar
        stem_w = max(1.5, w * 0.16)
        top = ky0 + h * 0.18
        bot = ky1 - h * 0.14
        if abs(x - cx) <= stem_w / 2 and top <= y <= bot:
            return True
        # chevron head pointing up: two arms meeting near top
        apex_y = ky0 + h * 0.16
        arm_len = h * 0.34
        arm_w = max(1.5, w * 0.16)
        if y <= apex_y + arm_len and y >= apex_y:
            t = (y - apex_y) / arm_len  # 0 at apex -> 1 at base
            half = t * (w * 0.30)
            return abs(x - cx) <= half + arm_w / 2 and abs(x - cx) >= max(0.0, half - arm_w / 2)
        return False

    for y in range(size):
        for x in range(size):
            i = (y * size + x) * 4
            if not in_round_rect(x, y, 0, 0, size, size, r_out):
                continue  # transparent outside
            # background: dark vertical gradient
            t = y / max(1, size - 1)
            bg = (30 + int(6 * t), 34 + int(8 * t), 42 + int(10 * t))
            col = bg
            kx0, ky0 = pad, pad
            kx1, ky1 = size - pad, size - pad
            if in_round_rect(x, y, kx0, ky0, kx1, ky1, max(1.0, size * 0.12)):
                col = (238, 240, 244)  # keycap
                if in_arrow(x, y, kx0, ky0, kx1, ky1):
                    col = (255, 122, 46)  # orange arrow
            px[i] = col[0]
            px[i + 1] = col[1]
            px[i + 2] = col[2]
            px[i + 3] = 255
    return bytes(px)


def png_bytes(size: int, raw: bytes) -> bytes:
    """Encode RGBA rows as a PNG."""
    stride = size * 4
    filtered = b"".join(b"\x00" + raw[y * stride:(y + 1) * stride] for y in range(size))
    def chunk(tag: bytes, data: bytes) -> bytes:
        return (struct.pack(">I", len(data)) + tag + data
                + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF))
    ihdr = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr)
            + chunk(b"IDAT", zlib.compress(filtered, 9)) + chunk(b"IEND", b""))


def ico_bytes(frames: list[tuple[int, bytes]]) -> bytes:
    """Build a .ico with PNG-compressed frames."""
    hdr = struct.pack("<HHH", 0, 1, len(frames))
    entries = b""
    body = b""
    for size, png in frames:
        w = size if size < 256 else 0
        entries += struct.pack("<BBBBHHII", w, w, 0, 0, 1, 32, len(png), 6 + len(entries) + len(body))
        body += png
    return hdr + entries + body


def main() -> None:
    os.makedirs(OUT_DIR, exist_ok=True)
    frames = []
    for size in (16, 24, 32, 48):
        raw = make_image(size)
        png = png_bytes(size, raw)
        frames.append((size, png))
        with open(os.path.join(OUT_DIR, f"icon_{size}.png"), "wb") as f:
            f.write(png)
    ico = ico_bytes(frames)
    path = os.path.join(OUT_DIR, "icon.ico")
    with open(path, "wb") as f:
        f.write(ico)
    print("wrote", path, len(ico), "bytes")


if __name__ == "__main__":
    main()
