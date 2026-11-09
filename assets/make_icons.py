"""Gera os ícones do app (sem dependências): ícone do Windows (.ico), do
macOS (.icns, via iconutil) e da janela (RGBA cru, 64x64).

Uso: python3 assets/make_icons.py
"""
import math, os, struct, subprocess, tempfile, zlib

HERE = os.path.dirname(os.path.abspath(__file__))
BG = (0x14, 0x17, 0x1C)
GREEN = (0x4A, 0xDE, 0x80)
ACCENT = (0x7C, 0xC4, 0xFF)


def rgba(S):
    """Quadrado arredondado escuro, dois anéis azuis e o centro verde."""
    f = S / 192.0
    small = S <= 32
    r = 40 * f
    rings = [(56 * f, max(1.6, 10 * f), 1.0)] if small else [(70 * f, 7 * f, 0.55), (52 * f, 7 * f, 0.85)]
    core = (44 if small else 34.5) * f
    c0 = (S - 1) / 2
    out = bytearray()
    for y in range(S):
        for x in range(S):
            cx = min(max(x, r), S - 1 - r)
            cy = min(max(y, r), S - 1 - r)
            a = max(0.0, min(1.0, r + 0.5 - math.hypot(x - cx, y - cy)))
            if a == 0:
                out += b"\0\0\0\0"
                continue
            c = BG
            d = math.hypot(x - c0, y - c0)
            for rad, w, alpha in rings:
                k = max(0.0, min(1.0, w / 2 + 0.5 - abs(d - rad))) * alpha
                c = tuple(int(c[i] * (1 - k) + ACCENT[i] * k) for i in range(3))
            k = max(0.0, min(1.0, core + 0.5 - d))
            c = tuple(int(c[i] * (1 - k) + GREEN[i] * k) for i in range(3))
            out += bytes(c) + bytes([int(a * 255)])
    return bytes(out)


def png(S, data):
    def chunk(t, d):
        return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)
    raw = b"".join(b"\0" + data[y * S * 4:(y + 1) * S * 4] for y in range(S))
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", S, S, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))


cache = {}
def png_of(S):
    if S not in cache:
        cache[S] = png(S, rgba(S))
    return cache[S]


# Windows: .ico com imagens PNG de 16 a 256.
sizes = [16, 24, 32, 48, 64, 128, 256]
imgs = [png_of(s) for s in sizes]
offset = 6 + 16 * len(sizes)
ico = struct.pack("<HHH", 0, 1, len(sizes))
for s, data in zip(sizes, imgs):
    ico += struct.pack("<BBBBHHII", s % 256, s % 256, 0, 0, 1, 32, len(data), offset)
    offset += len(data)
ico += b"".join(imgs)
open(os.path.join(HERE, "icon.ico"), "wb").write(ico)

# Janela (todas as plataformas): RGBA 64x64.
open(os.path.join(HERE, "icon-64.rgba"), "wb").write(rgba(64))
open(os.path.join(HERE, "icon-256.png"), "wb").write(png_of(256))

# macOS: .icns (precisa do iconutil, só no Mac).
with tempfile.TemporaryDirectory() as tmp:
    iconset = os.path.join(tmp, "AppIcon.iconset")
    os.mkdir(iconset)
    for base in [16, 32, 128, 256, 512]:
        open(os.path.join(iconset, f"icon_{base}x{base}.png"), "wb").write(png_of(base))
        open(os.path.join(iconset, f"icon_{base}x{base}@2x.png"), "wb").write(png_of(base * 2))
    try:
        subprocess.run(["iconutil", "-c", "icns", iconset, "-o", os.path.join(HERE, "AppIcon.icns")], check=True)
    except FileNotFoundError:
        print("iconutil não encontrado: AppIcon.icns não foi gerado")
print("ok")
