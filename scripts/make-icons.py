#!/usr/bin/env python3
"""Generate the full app icon set from one vector description.

Pure stdlib — there is no image library to depend on, and this way the icons
are reproducible from source rather than being opaque binaries in the tree.

Mark: a downward arrow landing in a tray, on an iOS-blue squircle. Rendered
once at high resolution, then box-filtered down to each size, so the small
ones stay legible.

    python3 scripts/make-icons.py
"""

import math
import os
import struct
import zlib

OUT = os.path.join(os.path.dirname(__file__), "..", "src-tauri", "icons")

MASTER = 1024  # rendered size before downsampling
SS = 2         # supersample factor at the master size


def rrect_sdf(px, py, cx, cy, hw, hh, r):
    """Signed distance to a rounded rect centred at (cx, cy). Negative inside."""
    qx = abs(px - cx) - (hw - r)
    qy = abs(py - cy) - (hh - r)
    return (math.hypot(max(qx, 0), max(qy, 0)) + min(max(qx, qy), 0)) - r


def in_tri(px, py, a, b, c):
    def sign(p, q, r):
        return (p[0] - r[0]) * (q[1] - r[1]) - (q[0] - r[0]) * (p[1] - r[1])

    d1, d2, d3 = sign((px, py), a, b), sign((px, py), b, c), sign((px, py), c, a)
    return not (((d1 < 0) or (d2 < 0) or (d3 < 0)) and ((d1 > 0) or (d2 > 0) or (d3 > 0)))


def render_master():
    """Render at MASTER*SS and box-filter to MASTER. Returns RGBA rows."""
    S = MASTER * SS
    acc = [[[0.0, 0.0, 0.0, 0.0] for _ in range(MASTER)] for _ in range(MASTER)]

    cx = cy = S / 2
    sq_h = S * 0.5 * 0.92        # half-extent of the squircle
    sq_r = S * 0.2235            # iOS superellipse approximation

    stem_hw, stem_top, stem_bot = S * 0.043, S * 0.215, S * 0.545
    head = [(cx - S * 0.125, S * 0.50), (cx + S * 0.125, S * 0.50), (cx, S * 0.665)]
    tray_y, tray_hw, tray_th = S * 0.745, S * 0.205, S * 0.042
    wall_h = S * 0.105

    for y in range(S):
        py = y + 0.5
        row = acc[y // SS]
        for x in range(S):
            px = x + 0.5
            if rrect_sdf(px, py, cx, cy, sq_h, sq_h, sq_r) > 0:
                continue

            t = py / S  # vertical gradient, lighter at the top
            r = int(10 + (0 - 10) * t)
            g = int(132 + (96 - 132) * t)
            b = int(255 + (223 - 255) * t)

            if (
                rrect_sdf(px, py, cx, (stem_top + stem_bot) / 2, stem_hw,
                          (stem_bot - stem_top) / 2, stem_hw) <= 0
                or in_tri(px, py, *head)
                or rrect_sdf(px, py, cx, tray_y, tray_hw, tray_th, tray_th) <= 0
                or rrect_sdf(px, py, cx - tray_hw + tray_th, tray_y - wall_h,
                             tray_th, wall_h, tray_th) <= 0
                or rrect_sdf(px, py, cx + tray_hw - tray_th, tray_y - wall_h,
                             tray_th, wall_h, tray_th) <= 0
            ):
                r = g = b = 255

            cell = row[x // SS]
            cell[0] += r
            cell[1] += g
            cell[2] += b
            cell[3] += 255

    n = SS * SS
    return [[(c[0] / n, c[1] / n, c[2] / n, c[3] / n) for c in row] for row in acc]


def downsample(rows, size):
    """Box-filter square RGBA rows down to `size`. Alpha-weighted, so the
    rounded edge does not pick up colour from transparent pixels."""
    src = len(rows)
    if src == size:
        return rows
    step = src / size
    out = []
    for y in range(size):
        y0, y1 = int(y * step), max(int(y * step) + 1, int((y + 1) * step))
        line = []
        for x in range(size):
            x0, x1 = int(x * step), max(int(x * step) + 1, int((x + 1) * step))
            r = g = b = a = 0.0
            count = 0
            for sy in range(y0, y1):
                for sx in range(x0, x1):
                    pr, pg, pb, pa = rows[sy][sx]
                    w = pa / 255.0
                    r += pr * w
                    g += pg * w
                    b += pb * w
                    a += pa
                    count += 1
            wsum = a / 255.0
            if wsum > 0:
                line.append((r / wsum, g / wsum, b / wsum, a / count))
            else:
                line.append((0.0, 0.0, 0.0, 0.0))
        out.append(line)
    return out


def png_bytes(rows):
    size = len(rows)
    raw = bytearray()
    for row in rows:
        raw.append(0)  # filter type 0
        for r, g, b, a in row:
            if a < 0.5:
                raw += bytes((0, 0, 0, 0))
            else:
                raw += bytes((
                    max(0, min(255, round(r))),
                    max(0, min(255, round(g))),
                    max(0, min(255, round(b))),
                    max(0, min(255, round(a))),
                ))

    def chunk(tag, data):
        c = tag + data
        return struct.pack(">I", len(data)) + c + struct.pack(">I", zlib.crc32(c))

    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(bytes(raw), 9))
        + chunk(b"IEND", b"")
    )


def write_ico(path, pngs):
    """Windows .ico. Vista and later read embedded PNG data directly, so each
    entry is just the PNG for that size."""
    count = len(pngs)
    header = struct.pack("<HHH", 0, 1, count)
    offset = 6 + 16 * count
    entries, blobs = b"", b""
    for size, data in pngs:
        entries += struct.pack(
            "<BBBBHHII",
            size if size < 256 else 0,  # 0 means 256
            size if size < 256 else 0,
            0, 0, 1, 32, len(data), offset,
        )
        blobs += data
        offset += len(data)
    open(path, "wb").write(header + entries + blobs)


def write_icns(path, entries):
    """macOS .icns. Each element is an OSType followed by a big-endian length
    that includes its own 8-byte header."""
    body = b""
    for ostype, data in entries:
        body += ostype + struct.pack(">I", len(data) + 8) + data
    open(path, "wb").write(b"icns" + struct.pack(">I", len(body) + 8) + body)


def main():
    os.makedirs(OUT, exist_ok=True)
    print(f"rendering master at {MASTER}x{MASTER} (supersampled {SS}x)...")
    master = render_master()

    sizes = [16, 32, 48, 64, 128, 256, 512, 1024]
    scaled = {}
    for size in sizes:
        scaled[size] = png_bytes(downsample(master, size))
        print(f"  {size}x{size}")

    # Tauri's expected PNG names.
    for name, size in [
        ("32x32.png", 32),
        ("128x128.png", 128),
        ("128x128@2x.png", 256),
        ("icon.png", 512),
    ]:
        open(os.path.join(OUT, name), "wb").write(scaled[size])
        print(f"  wrote {name}")

    write_ico(os.path.join(OUT, "icon.ico"),
              [(s, scaled[s]) for s in (16, 32, 48, 64, 128, 256)])
    print("  wrote icon.ico")

    write_icns(os.path.join(OUT, "icon.icns"), [
        (b"ic07", scaled[128]),    # 128x128
        (b"ic08", scaled[256]),    # 256x256
        (b"ic09", scaled[512]),    # 512x512
        (b"ic10", scaled[1024]),   # 512x512@2x
        (b"ic11", scaled[32]),     # 16x16@2x
        (b"ic12", scaled[64]),     # 32x32@2x
        (b"ic13", scaled[256]),    # 128x128@2x
        (b"ic14", scaled[512]),    # 256x256@2x
    ])
    print("  wrote icon.icns")


if __name__ == "__main__":
    main()
