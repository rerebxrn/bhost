"""Icônes de BHOST : le B + un point, sur une tuile, une par thème.

Lancer : python docs/logo/icone.py   (il faut : pip install cairosvg pillow)
Écrit dans src-tauri/icons/ : icon.ico (Azur clair, aussi celle du programme),
azur-sombre.ico, ambre-clair.ico, ambre-sombre.ico, et les PNG / .icns demandés par Tauri.
"""
import io
import pathlib
import struct

import cairosvg
from PIL import Image

# Le B, en formes (coordonnées sur 1000 ; il occupe x 30→500, y -600→0).
B = "M30 0Q38 -149 38.0 -300.0Q38 -451 30 -600H224Q299 -600 347.0 -593.5Q395 -587 422.0 -569.5Q449 -552 459.0 -521.5Q469 -491 469 -444Q469 -379 450.5 -347.5Q432 -316 384 -306Q419 -303 441.5 -294.0Q464 -285 477.0 -268.0Q490 -251 495.0 -225.5Q500 -200 500 -162Q500 -110 490.0 -78.5Q480 -47 451.5 -29.5Q423 -12 370.5 -6.0Q318 0 234 0ZM327 -391Q354 -391 361.5 -396.0Q369 -401 369 -411Q369 -421 361.5 -426.0Q354 -431 327 -431L164 -430Q164 -420 163.5 -410.0Q163 -400 163 -390ZM346 -171Q378 -171 389.0 -177.0Q400 -183 400 -197Q400 -210 389.0 -215.5Q378 -221 346 -221L163 -220Q163 -207 163.5 -195.0Q164 -183 164 -170Z"

THEMES = {
    "azur-clair": dict(tuile="#ffffff", b="#0f1f3d", point="#1b5bcc", bord="#0f1f3d", bo=0.14),
    "azur-sombre": dict(tuile="#151a25", b="#ecf0f8", point="#8fb2ee", bord="#ffffff", bo=0.16),
    "ambre-clair": dict(tuile="#f6f2ec", b="#2b2427", point="#7a5b3a", bord="#433a40", bo=0.16),
    "ambre-sombre": dict(tuile="#1c1c2c", b="#ecebf4", point="#93a8f4", bord="#ffffff", bo=0.16),
}
MARGE = 30
TAILLES_ICO = [16, 20, 24, 32, 40, 48, 64, 128, 256]


def svg(theme, petit=False):
    """La tuile en SVG. En tout petit (≤ 24 px), le point disparaît et le B grossit un peu."""
    c = THEMES[theme]
    echelle = 0.92 if petit else 0.86
    # Le B (sans le point) reste pile au centre de la tuile (centre : 365, -235).
    x, y = 365 - 265 * echelle, -235 + 300 * echelle
    point = "" if petit else f'<circle cx="524" cy="-566" r="68" fill="{c["point"]}" stroke="{c["tuile"]}" stroke-width="34"/>'
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{-135 - MARGE} {-735 - MARGE} {1000 + 2 * MARGE} {1000 + 2 * MARGE}">'
        f'<rect x="-128" y="-728" width="986" height="986" rx="222" fill="{c["tuile"]}" stroke="{c["bord"]}" stroke-opacity="{c["bo"]}" stroke-width="14"/>'
        f'<g transform="translate({x:.1f} {y:.1f}) scale({echelle})"><path fill="{c["b"]}" d="{B}"/>{point}</g></svg>'
    )


def png(theme, taille):
    return cairosvg.svg2png(bytestring=svg(theme, taille <= 24).encode(), output_width=taille, output_height=taille)


def ico(theme):
    """Un .ico avec chaque taille rangée en PNG (ce que lit src-tauri/src/icone.rs)."""
    images = [png(theme, n) for n in TAILLES_ICO]
    fiches, donnees = b"", b""
    debut = 6 + 16 * len(TAILLES_ICO)
    for n, d in zip(TAILLES_ICO, images):
        fiches += struct.pack("<BBBBHHII", n % 256, n % 256, 0, 0, 1, 32, len(d), debut + len(donnees))
        donnees += d
    return struct.pack("<HHH", 0, 1, len(TAILLES_ICO)) + fiches + donnees


if __name__ == "__main__":
    sortie = pathlib.Path(__file__).resolve().parents[2] / "src-tauri" / "icons"
    (sortie / "icon.ico").write_bytes(ico("azur-clair"))
    for t in ("azur-sombre", "ambre-clair", "ambre-sombre"):
        (sortie / f"{t}.ico").write_bytes(ico(t))
    pngs = {
        "32x32.png": 32, "128x128.png": 128, "128x128@2x.png": 256, "icon.png": 512, "StoreLogo.png": 50,
        "Square30x30Logo.png": 30, "Square44x44Logo.png": 44, "Square71x71Logo.png": 71, "Square89x89Logo.png": 89,
        "Square107x107Logo.png": 107, "Square142x142Logo.png": 142, "Square150x150Logo.png": 150,
        "Square284x284Logo.png": 284, "Square310x310Logo.png": 310,
    }
    for nom, n in pngs.items():
        (sortie / nom).write_bytes(png("azur-clair", n))
    Image.open(io.BytesIO(png("azur-clair", 1024))).save(sortie / "icon.icns")
