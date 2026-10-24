"""Generate SmartBlink icon assets (assets/icon.ico with 16/24/32/48/256 px PNG frames)
directly from the canonical vector geometry of assets/smartblink-tray.svg.

Pure Python stdlib (math, struct, zlib).
"""
import math
import os
import struct
import zlib

OUT_DIR = os.path.join(os.path.dirname(__file__), "..", "assets")

# ViewBox of smartblink-tray.svg / smartblink-logo.svg: x=10, y=20, w=190, h=200
VB_X = 10.0
VB_Y = 20.0
VB_W = 190.0
VB_H = 200.0


def sample_cubic(p0, p1, p2, p3, steps=48):
    pts = []
    for i in range(steps + 1):
        t = i / steps
        mt = 1.0 - t
        x = (
            mt * mt * mt * p0[0]
            + 3.0 * mt * mt * t * p1[0]
            + 3.0 * mt * t * t * p2[0]
            + t * t * t * p3[0]
        )
        y = (
            mt * mt * mt * p0[1]
            + 3.0 * mt * mt * t * p1[1]
            + 3.0 * mt * t * t * p2[1]
            + t * t * t * p3[1]
        )
        pts.append((x, y))
    return pts


def build_polylines():
    # Path 1 (L-stem): M 100 50 L 100 192 L 164 192
    stem = [(100.0, 50.0), (100.0, 192.0), (164.0, 192.0)]

    # Path 2 (connector): M 92 102 L 100 94 C 118 78, 148.8 59.6, 144 52
    conn = [(92.0, 102.0)] + sample_cubic(
        (100.0, 94.0), (118.0, 78.0), (148.8, 59.6), (144.0, 52.0), steps=32
    )

    # Path 3 (S-curve):
    # M 144 52
    # C 138.6 43.8, 120 48, 100 62
    # C 80 76, 50 95.2, 50 109
    # C 50 121.6, 74 118, 100 121
    # C 124 124, 152 123.8, 152 136.4
    # C 152 150.2, 126 168, 100 192
    s_segments = [
        ((144.0, 52.0), (138.6, 43.8), (120.0, 48.0), (100.0, 62.0)),
        ((100.0, 62.0), (80.0, 76.0), (50.0, 95.2), (50.0, 109.0)),
        ((50.0, 109.0), (50.0, 121.6), (74.0, 118.0), (100.0, 121.0)),
        ((100.0, 121.0), (124.0, 124.0), (152.0, 123.8), (152.0, 136.4)),
        ((152.0, 136.4), (152.0, 150.2), (126.0, 168.0), (100.0, 192.0)),
    ]
    s_curve = []
    for idx, (p0, p1, p2, p3) in enumerate(s_segments):
        pts = sample_cubic(p0, p1, p2, p3, steps=40)
        if idx > 0:
            pts = pts[1:]
        s_curve.extend(pts)

    def to_segments(poly):
        segs = []
        for i in range(len(poly) - 1):
            ax, ay = poly[i]
            bx, by = poly[i + 1]
            dx = bx - ax
            dy = by - ay
            len_sq = dx * dx + dy * dy
            segs.append((ax, ay, dx, dy, len_sq))
        return segs

    bg_segs = to_segments(stem) + to_segments(conn) + to_segments(s_curve)
    fg_segs = to_segments(s_curve)
    return bg_segs, fg_segs


BG_SEGS, FG_SEGS = build_polylines()


def min_dist_to_segs(px: float, py: float, segs) -> float:
    best_sq = 1e18
    for ax, ay, dx, dy, len_sq in segs:
        if len_sq > 1e-12:
            t = ((px - ax) * dx + (py - ay) * dy) / len_sq
            if t < 0.0:
                t = 0.0
            elif t > 1.0:
                t = 1.0
            cx = ax + t * dx
            cy = ay + t * dy
        else:
            cx, cy = ax, ay
        ddx = px - cx
        ddy = py - cy
        d2 = ddx * ddx + ddy * ddy
        if d2 < best_sq:
            best_sq = d2
    return math.sqrt(best_sq)


def smooth_coverage(dist: float, radius: float, aa_width: float) -> float:
    half_aa = 0.5 * aa_width
    if dist <= radius - half_aa:
        return 1.0
    if dist >= radius + half_aa:
        return 0.0
    # Smoothstep falloff across 1 pixel
    t = (dist - (radius - half_aa)) / aa_width
    return 1.0 - (t * t * (3.0 - 2.0 * t))


def blend_over(dst_r, dst_g, dst_b, dst_a, src_r, src_g, src_b, src_a):
    if src_a <= 0.0:
        return dst_r, dst_g, dst_b, dst_a
    out_a = src_a + dst_a * (1.0 - src_a)
    if out_a <= 1e-8:
        return 0.0, 0.0, 0.0, 0.0
    out_r = (src_r * src_a + dst_r * dst_a * (1.0 - src_a)) / out_a
    out_g = (src_g * src_a + dst_g * dst_a * (1.0 - src_a)) / out_a
    out_b = (src_b * src_a + dst_b * dst_a * (1.0 - src_a)) / out_a
    return out_r, out_g, out_b, out_a


def make_tray_image(size: int) -> bytes:
    """Render smartblink-tray.svg (stroke-width 18 bg, 20 fg) into RGBA bytes."""
    px = bytearray(size * size * 4)
    # Uniform scale (preserveAspectRatio="xMidYMid meet" on viewBox 10 20 190 200)
    scale = min(size / VB_W, size / VB_H)
    ox = (size - VB_W * scale) * 0.5 - VB_X * scale
    oy = (size - VB_H * scale) * 0.5 - VB_Y * scale
    aa_vb = 1.15 / scale  # 1.15 screen pixel anti-aliasing width in viewBox units

    bg_r, bg_g, bg_b = 0x8B / 255.0, 0x95 / 255.0, 0xA5 / 255.0
    fg_r, fg_g, fg_b = 0x00 / 255.0, 0xE5 / 255.0, 0xFF / 255.0

    # Slightly boost stroke weight at tiny 16px size so tray icon stays bold
    boost = 1.12 if size <= 16 else (1.05 if size <= 24 else 1.0)
    r_bg = 9.0 * boost
    r_fg = 10.0 * boost

    for y in range(size):
        vy = (y + 0.5 - oy) / scale
        for x in range(size):
            vx = (x + 0.5 - ox) / scale
            d_bg = min_dist_to_segs(vx, vy, BG_SEGS)
            d_fg = min_dist_to_segs(vx, vy, FG_SEGS)

            a_bg = smooth_coverage(d_bg, r_bg, aa_vb) * 0.55
            a_fg = smooth_coverage(d_fg, r_fg, aa_vb)

            r, g, b, a = blend_over(bg_r, bg_g, bg_b, a_bg, fg_r, fg_g, fg_b, a_fg)
            i = (y * size + x) * 4
            px[i] = min(255, max(0, int(round(r * 255.0))))
            px[i + 1] = min(255, max(0, int(round(g * 255.0))))
            px[i + 2] = min(255, max(0, int(round(b * 255.0))))
            px[i + 3] = min(255, max(0, int(round(a * 255.0))))
    return bytes(px)


def png_bytes(size: int, raw: bytes) -> bytes:
    """Encode RGBA rows as a PNG."""
    stride = size * 4
    filtered = b"".join(b"\x00" + raw[y * stride : (y + 1) * stride] for y in range(size))

    def chunk(tag: bytes, data: bytes) -> bytes:
        return (
            struct.pack(">I", len(data))
            + tag
            + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        )

    ihdr = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", ihdr)
        + chunk(b"IDAT", zlib.compress(filtered, 9))
        + chunk(b"IEND", b"")
    )


def dib_bytes(size: int, raw: bytes) -> bytes:
    """Encode RGBA rows as a standard ICO 32-bit BGRA DIB (BITMAPINFOHEADER + bottom-up XOR + 1-bit AND)."""
    # BITMAPINFOHEADER (40 bytes): height is 2 * size (XOR + AND masks)
    bih = struct.pack(
        "<IIIHHIIIIII",
        40,
        size,
        size * 2,
        1,
        32,
        0,  # BI_RGB
        size * size * 4,
        0,
        0,
        0,
        0,
    )
    xor_mask = bytearray(size * size * 4)
    and_row_bytes = ((size + 31) // 32) * 4
    and_mask = bytearray(and_row_bytes * size)

    for y in range(size):
        src_y = size - 1 - y  # bottom-up
        for x in range(size):
            si = (src_y * size + x) * 4
            di = (y * size + x) * 4
            r, g, b, a = raw[si], raw[si + 1], raw[si + 2], raw[si + 3]
            xor_mask[di] = b
            xor_mask[di + 1] = g
            xor_mask[di + 2] = r
            xor_mask[di + 3] = a
            if a < 16:
                byte_idx = y * and_row_bytes + (x >> 3)
                and_mask[byte_idx] |= 0x80 >> (x & 7)

    return bih + bytes(xor_mask) + bytes(and_mask)


def ico_bytes(frames: list[tuple[int, bytes]]) -> bytes:
    """Build a .ico with DIB frames (<256) and PNG frame (256)."""
    hdr = struct.pack("<HHH", 0, 1, len(frames))
    entries = b""
    body = b""
    for size, payload in frames:
        w = size if size < 256 else 0
        entries += struct.pack(
            "<BBBBHHII",
            w,
            w,
            0,
            0,
            1,
            32,
            len(payload),
            6 + 16 * len(frames) + len(body),
        )
        body += payload
    return hdr + entries + body


def main() -> None:
    os.makedirs(OUT_DIR, exist_ok=True)
    frames = []
    for size in (16, 24, 32, 48, 64, 256):
        raw = make_tray_image(size)
        png = png_bytes(size, raw)
        payload = dib_bytes(size, raw) if size < 256 else png
        frames.append((size, payload))
        if size in (16, 24, 32, 48):
            with open(os.path.join(OUT_DIR, f"icon_{size}.png"), "wb") as f:
                f.write(png)
    ico = ico_bytes(frames)
    path = os.path.join(OUT_DIR, "icon.ico")
    with open(path, "wb") as f:
        f.write(ico)
    print("wrote", path, len(ico), "bytes")


if __name__ == "__main__":
    main()
