# Les icônes de serveur de BHOST : des cubes isométriques en pixels, 6 × 6 par face.
# Rien ne dépasse du cube. Chaque icône = trois grilles (dessus, façade gauche, façade droite)
# écrites en lettres ; chaque lettre est une matière (ombrée selon la face) ou une couleur fixe.
#
# Repères des grilles (6 lignes de 6 lettres) :
#   dessus  : ligne 0 = fond ; colonne 0 = coin gauche-arrière… la ligne 5 touche la façade gauche,
#             la colonne 5 touche la façade droite (colonne 5 ligne 0 = coin droit).
#   gauche  : ligne 0 = haut ; colonne 0 = coin gauche, colonne 5 = arête de devant.
#   droite  : ligne 0 = haut ; colonne 0 = arête de devant, colonne 5 = coin droit.
# Utilisation : python3 icones.py  → dans le dossier « sortie » :
#     planche.png         la planche de toutes les icônes (fond clair et sombre, avec la taille de la barre latérale)
#     app/<clé>.png       → à copier dans src/assets/icones/ (ce que l'appli affiche, 192 px)
#     jeu/<clé>.png       → à copier dans src-tauri/icones-jeu/ (l'icône dans la liste des serveurs de Minecraft, 64 px)
#     liste.json          les clés, noms et groupes (à reporter dans src/theme/icones.ts et ICONES de serveurs.rs)
# Pourquoi des PNG et plus des SVG (18/11) : en SVG, chaque pixel est un losange ; en petit, le navigateur
# adoucit chaque bord séparément et des fines lignes (une grille) apparaissent entre les pixels.
# Ici, on peint en très grand sans adoucir (aucun joint possible), puis on réduit d'un coup : pas de grille.
import colorsys, math, os
from PIL import Image, ImageDraw, ImageFont

N = 6
# La lumière, rangée par rangée (0 = ombre, 1 = reflet), comme le logo.
LUM = {
    "t": [0.62, 0.72, 0.84, 0.90, 0.70, 0.82],
    "g": [0.60, 0.53, 0.49, 0.45, 0.40, 0.34],
    "d": [0.36, 0.31, 0.27, 0.24, 0.21, 0.16],
}
# Métal brossé (serveurs) : plus de contraste.
LUM_METAL = {
    "t": [0.62, 0.76, 0.90, 1.00, 0.68, 0.86],
    "g": [0.74, 0.52, 0.46, 0.42, 0.36, 0.26],
    "d": [0.42, 0.28, 0.24, 0.21, 0.17, 0.10],
}
METAUX = {
    "acier":  ["#15181D", "#262B33", "#3A414C", "#525B68", "#6E7885", "#8D97A4", "#AEB7C2", "#CDD4DC", "#E8ECF1", "#FBFCFD"],
    "dore":   ["#1E1208", "#3A230E", "#5A3918", "#7E5325", "#A17035", "#C08E4C", "#DBAF6E", "#EFCF98", "#FBE9C6", "#FFF9EC"],
    "titane": ["#1A1714", "#2B2622", "#413A34", "#5B5249", "#786D62", "#978B7E", "#B7AB9D", "#D5CBBE", "#EDE6DB", "#FCFAF6"],
    "noir":   ["#060708", "#0C0E11", "#14171B", "#1C2026", "#262B32", "#323840", "#424A55", "#5A636F", "#7E8794", "#AEB6C1"],
    "orrose": ["#1F0F0F", "#3B1D1C", "#5C2E2C", "#7F423F", "#A05A55", "#BC7670", "#D4958E", "#E7B6AF", "#F5D6D1", "#FDF0EE"],
}


def hexrgb(h):
    return tuple(int(h[i:i + 2], 16) / 255 for i in (1, 3, 5))


def rgbhex(r, g, b):
    return "#%02X%02X%02X" % tuple(max(0, min(255, round(x * 255))) for x in (r, g, b))


def ton(base, t):
    """Comme l'appli : plus sombre et un peu plus froid dans l'ombre, plus clair et plus chaud au soleil."""
    r, g, b = hexrgb(base)
    h, l, s = colorsys.rgb_to_hls(r, g, b)
    h *= 360
    vers = lambda c, k: h + (((c - h + 540) % 360) - 180) * k
    teinte = vers(240, (0.55 - t) * 0.25) if t < 0.55 else vers(60, (t - 0.55) * 0.18)
    clarte = min(0.95, max(0.08, l + (t - 0.6) * 0.78))
    sat = min(s, 0.92) * (0.55 + 0.3 * t)
    return rgbhex(*colorsys.hls_to_rgb((teinte % 360) / 360, clarte, sat))


def bord(face, i):
    """Les arêtes tournées vers nous accrochent la lumière ; celles du fond sont un peu plus sombres."""
    if face == "t":
        return 0.08 if i == N - 1 else (-0.04 if i == 0 else 0)
    if face == "g":
        return 0.09 if i == N - 1 else (-0.03 if i == 0 else 0)
    return 0.06 if i == 0 else (-0.03 if i == N - 1 else 0)


# Une lettre de la palette : ("m", couleur) matière ombrée ; ("m", couleur, décalage) plus clair/sombre ;
# ("x", couleur) couleur fixe (yeux, voyants) ; ("metal", nom, décalage) métal brossé.
def peindre(icone):
    pal = icone["pal"]
    grilles = {}
    for f in "tgd":
        lignes = icone.get(f) or [icone.get("fond", "M") * N] * N
        assert len(lignes) == N and all(len(l) == N for l in lignes), (icone["nom"], f)
        g = []
        for r, ligne in enumerate(lignes):
            rang = []
            for i, ch in enumerate(ligne):
                p = pal[ch]
                if p[0] == "vide":
                    rang.append(None)
                elif p[0] == "x":
                    rang.append(p[1])
                elif p[0] == "metal":
                    rampe = METAUX[p[1]]
                    t = LUM_METAL[f][r] + bord(f, i) + (p[2] if len(p) > 2 else 0)
                    rang.append(rampe[max(0, min(9, round(t * 9)))])
                elif p[0] == "v":
                    # Couleur vive : ombrée, mais beaucoup moins (petits détails qui doivent rester lisibles).
                    t = 0.6 + (LUM[f][r] - 0.6) * (p[3] if len(p) > 3 else 0.35) + (p[2] if len(p) > 2 else 0)
                    rang.append(ton(p[1], t))
                else:
                    t = LUM[f][r] + bord(f, i) + (p[2] if len(p) > 2 else 0)
                    rang.append(ton(p[1], t))
            g.append(rang)
        grilles[f] = g
    return grilles


def quads(grilles, e):
    ax, ay = e * math.cos(math.radians(30)), e * 0.5
    faces = [
        ((N * ax, 0.0), (ax, ay), (-ax, ay), grilles["t"]),
        ((0.0, N * ay), (ax, ay), (0.0, e), grilles["g"]),
        ((N * ax, 2 * N * ay), (ax, -ay), (0.0, e), grilles["d"]),
    ]
    out = []
    for P, u, v, c in faces:
        for r in range(N):
            for i in range(N):
                ox, oy = P[0] + i * u[0] + r * v[0], P[1] + i * u[1] + r * v[1]
                out.append(([(ox, oy), (ox + u[0], oy + u[1]), (ox + u[0] + v[0], oy + u[1] + v[1]), (ox + v[0], oy + v[1])], c[r][i]))
    return out, (2 * N * ax, 2 * N * ay + N * e)


def png(icone, taille, marge=0.06):
    S = max(4, 1536 // taille)
    grilles = peindre(icone)
    _, (w1, h1) = quads(grilles, 1.0)
    boite = taille * S * (1 - 2 * marge)
    e = min(boite / w1, boite / h1)
    qs, (w, h) = quads(grilles, e)
    dx, dy = (taille * S - w) / 2, (taille * S - h) / 2
    im = Image.new("RGBA", (taille * S, taille * S), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    for pts, col in qs:
        if col is None:
            continue
        d.polygon([(x + dx, y + dy) for x, y in pts], fill=col, outline=col)
    return im.resize((taille, taille), Image.LANCZOS)


# ---------------------------------------------------------------- les icônes

ICONES = []


def icone(groupe, nom, cle, pal, t=None, g=None, d=None, fond="M"):
    ICONES.append(dict(groupe=groupe, nom=nom, cle=cle, pal=pal, t=t, g=g, d=d, fond=fond))


NOIR = ("x", "#1E2127")
BLANC = ("x", "#F7F6F2")

# Les 10 couleurs d'origine.
for cle, nom, base in [
    ("graphite", "Graphite", "#8A8D96"), ("bleu", "Bleu", "#3D8BF2"), ("indigo", "Indigo", "#6463DB"),
    ("violet", "Violet", "#A866D9"), ("rose", "Rose", "#EC5B87"), ("rouge", "Rouge", "#E8584E"),
    ("orange", "Orange", "#F0943A"), ("jaune", "Jaune", "#EBC23F"), ("vert", "Vert", "#4CBF6B"),
    ("menthe", "Menthe", "#45BFB4"),
]:
    icone("Couleurs", nom, cle, {"M": ("m", base)})

# Les 3 serveurs (les anciens logos) : deux aérations et deux voyants à gauche, une aération à droite.
for metal, nom in [("acier", "Serveur acier"), ("dore", "Serveur doré"), ("titane", "Serveur titane"),
                   ("noir", "Serveur noir"), ("orrose", "Serveur or rose")]:
    icone("Serveurs", nom, "serveur-" + metal,
          {"M": ("metal", metal), "A": ("metal", metal, -1), "V": ("x", "#5ED68A"), "O": ("x", "#F2C14E")},
          g=["MMMMMM", "MAAAAM", "MMMMMM", "MAAAAM", "MVMOMM", "MMMMMM"],
          d=["MMMMMM", "MMMMMM", "MAAAAM", "MMMMMM", "MMMMMM", "MMMMMM"])

# Cadeaux : ruban en croix sur le dessus, qui descend sur les deux façades, nœud plus clair au centre.
DESSUS_CADEAU = ["BBRRBB", "BBRRBB", "RRKKRR", "RRKKRR", "BBRRBB", "BBRRBB"]
FACE_CADEAU = ["BBRRBB"] * 6
for cle, nom, boite, ruban in [("cadeau-rouge", "Cadeau rouge", "#E0443C", "#F2C14E"),
                               ("cadeau-bleu", "Cadeau bleu", "#3D8BF2", "#F28DB0"),
                               ("cadeau-vert", "Cadeau de Noël", "#33A05A", "#E0443C")]:
    icone("Fête", nom, cle,
          {"B": ("m", boite), "R": ("m", ruban), "K": ("m", ruban, 0.12)},
          t=DESSUS_CADEAU, g=FACE_CADEAU, d=FACE_CADEAU, fond="B")

icone("Fête", "Gâteau d'anniversaire", "gateau",
      {"P": ("m", "#F28DB0"), "S": ("m", "#E7BE85"), "C": ("m", "#FFF4E8"), "y": ("v", "#F6CE3C"), "b": ("v", "#5BA8F5"),
       "r": ("v", "#D7263D"), "h": ("x", "#FFC2CC")},
      t=["PPPPPP", "PPPPbP", "PPhrPP", "PPrrPP", "PyPPPP", "PPPPPP"],
      g=["PPPPPP", "PSPPSP", "SSSPSS", "CCCCCC", "SSSSSS", "SSSSSS"],
      d=["PPPPPP", "PPSPPS", "SPSSPS", "CCCCCC", "SSSSSS", "SSSSSS"])
icone("Fête", "Gâteau au chocolat", "gateau-chocolat",
      {"D": ("m", "#5A3420"), "S": ("m", "#9A633F"), "C": ("m", "#FFF4E6"), "r": ("v", "#E0303F"), "v": ("v", "#3E9E48")},
      t=["DDDDDD", "DDDDDD", "DDvDDD", "DDrrDD", "DDrrDD", "DDDDDD"],
      g=["DDDDDD", "DSDDSD", "SSSDSS", "CCCCCC", "SSSSSS", "SSSSSS"],
      d=["DDDDDD", "DDSDDS", "SDSSDS", "CCCCCC", "SSSSSS", "SSSSSS"])

# Gourmandises.
icone("Gourmandises", "Fromage", "fromage",
      {"M": ("m", "#F4C542"), "H": ("m", "#F4C542", -0.24), "h": ("m", "#F4C542", -0.12)},
      t=["MMMMMM", "MMMMMM", "MMMHhM", "MMMMMM", "MhMMMM", "MMMMMM"],
      g=["MMMMMM", "MHhMMM", "MhhMMM", "MMMMMM", "MMMMHM", "MMMMMM"],
      d=["MMMMMM", "MMMMMM", "MMHhMM", "MMMMMM", "MhMMMM", "MMMMMM"])


# Objets.
icone("Objets", "Carton", "carton",
      {"M": ("m", "#C6935B"), "T": ("m", "#E2BF8C"), "w": ("m", "#FAF7F0"), "l": ("v", "#9A8F84"), "r": ("v", "#D9443A")},
      t=["MMTTMM"] * 6,
      g=["MMTTMM", "MMTTMM", "MMMMMM", "MMMMMM", "MMMMMM", "MMMMMM"],
      d=["MMMMMM", "MMMMMM", "MwwwwM", "MwllwM", "MwrwwM", "MMMMMM"])
icone("Objets", "Dé", "de",
      {"M": ("m", "#F6F5F1"), "c": ("m", "#F6F5F1", -0.12), "r": ("x", "#D93A3A"), "d": NOIR},
      t=["cMMMMc", "MMMMMM", "MMrrMM", "MMrrMM", "MMMMMM", "cMMMMc"],
      g=["cMMMMc", "MdMMdM", "MMMMMM", "MMMMMM", "MdMMdM", "cMMMMc"],
      d=["cMMMMc", "MMMMdM", "MMMMMM", "MMMMMM", "MdMMMM", "cMMMMc"])
# Caméra : un gros objectif rond sur la façade, le déclencheur et le flash sur le dessus, un voyant rouge.
icone("Objets", "Caméra", "camera",
      {"M": ("m", "#3A3E47"), "s": ("m", "#3A3E47", -0.1), "R": ("metal", "acier", 0.12), "g": ("v", "#2A3566"),
       "G": ("v", "#4B5FA8"), "w": ("x", "#EAF1FF"), "F": ("v", "#F4F4F0"), "S": ("v", "#E0443C"), "l": ("x", "#FF4F45")},
      t=["MMMMMM", "MFFMMM", "MMMMMM", "MMMMMM", "MMMMSM", "MMMMMM"],
      g=["MMMMMM", "MMRRMM", "MRwGRM", "MRGgRM", "MMRRMM", "MMMMMM"],
      d=["MMMMMM", "MMMMlM", "MssssM", "MssssM", "MssssM", "MMMMMM"])
icone("Nature", "Aquarium", "aquarium",
      {"F": ("m", "#D4E9F0"), "a": ("m", "#57B4E6"), "A": ("m", "#2F95D8"), "o": ("x", "#F68A2E"), "T": ("x", "#D9661A"),
       "e": ("x", "#1E2127"), "w": ("x", "#E8F8FF"), "v": ("v", "#3FAE58"), "y": ("m", "#E2C994")},
      t=["FFFFFF", "FaaaaF", "FaaaaF", "FaaaaF", "FaaaaF", "FFFFFF"],
      g=["FFFFFF", "AAAAwA", "AeooTA", "AAoAAA", "AAAAAv", "yyyyvy"],
      d=["FFFFFF", "AAAAAA", "AAAwAA", "AAAAAA", "AvAAAA", "yvyyyy"])


# Bloc d'herbe (dessin à nous) : herbe sur le dessus avec deux touffes plus claires, qui retombe en bord irrégulier sur la terre.
icone("Nature", "Bloc d'herbe", "herbe",
      {"V": ("m", "#5DB846"), "v": ("m", "#5DB846", 0.1), "f": ("m", "#5DB846", -0.1), "T": ("m", "#9A6A43"), "t": ("m", "#9A6A43", -0.14)},
      t=["VVVVfV", "VvVVVV", "VVVfVV", "VVVVVv", "fVvVVV", "VVVVVV"],
      g=["VVVVVV", "VTVVTV", "TTTVTT", "TTTTTT", "TtTTTT", "TTTTtT"],
      d=["VVVVVV", "VVTVVT", "TVTTTT", "TTTTtT", "TtTTTT", "TTTTTT"])
# Hiver : une épaisse couche de neige, bord arrondi qui coule un peu sur les côtés, deux flocons.
icone("Nature", "Hiver", "hiver",
      {"M": ("m", "#3E74CF"), "W": ("m", "#EEF5FD"), "w": ("v", "#C2D7F0"), "f": ("x", "#EAF4FF")},
      t=["wwwwww", "wWWWWW", "wWWWWW", "wWWWWW", "wWWWWW", "wWWWWW"],
      g=["WWWWWW", "WWWWWW", "WWMWWM", "MMMWMM", "MMMMMM", "MfMMMM"],
      d=["WWWWWW", "WWWWWW", "MWWWMM", "MMWMMM", "MMMMMM", "MMMfMM"])

icone("Nature", "Grenouille", "grenouille",
      {"M": ("m", "#5DBB4A"), "w": ("x", "#FFFFFF"), "d": NOIR, "p": ("x", "#F59A86")},
      g=["MMMMMM", "wdMMwd", "wwMMww", "pMMMMp", "MddddM", "MMMMMM"])


# Ajoutées le 18/11 (choisies par Thomas parmi les propositions).
icone("Gourmandises", "Pain de mie", "pain",
      {"C": ("m", "#B9732F"), "c": ("m", "#B9732F", 0.08), "M": ("m", "#F8E8C4"), "h": ("m", "#F8E8C4", -0.1)},
      t=["CCCCCC", "CccccC", "CccccC", "CccccC", "CccccC", "CCCCCC"],
      g=["CCCCCC", "CMMMMC", "CMMhMC", "CMMMMC", "CMMMMC", "CCCCCC"],
      d=["CCCCCC"] * 6)
# Guimauve bicolore (blanc dessus, rose dessous), comme en confiserie. Toute moelleuse
# : bords plus clairs que le centre de chaque face (effet coussin), couleurs douces, aucun détail inutile.
def guimauve():
    def face(haut, bas, vides):
        lignes = []
        for r in range(N):
            ligne = ""
            for i in range(N):
                if (r, i) in vides:
                    ligne += "o"
                    continue
                bord = r in (0, N - 1) or i in (0, N - 1)
                couleur = haut if r < 3 else bas
                ligne += couleur.upper() if bord else couleur
            lignes.append(ligne)
        return lignes
    t = face("b", "b", set())
    g = face("b", "r", set())
    d = face("b", "r", set())
    return t, g, d


T, G, D = guimauve()
icone("Gourmandises", "Guimauve", "guimauve",
      {"b": ("v", "#EEE6E2", 0, 0.6), "B": ("v", "#EEE6E2", 0.05, 0.6), "r": ("v", "#F3A6C0", 0, 0.6),
       "R": ("v", "#F3A6C0", 0.05, 0.6)},
      t=T, g=G, d=D)
icone("Gourmandises", "Bento", "bento",
      {"K": ("m", "#25262B"), "r": ("v", "#C8323A"), "W": ("v", "#F6F4EE"), "S": ("v", "#F28A6A"), "G": ("v", "#4CAF50"),
       "Y": ("v", "#F2C83E")},
      t=["KKKKKK", "KWWSSK", "KWWSSK", "KKKKKK", "KGGYYK", "KGGYYK"],
      g=["KKKKKK", "rrrrrr", "KKKKKK", "KKKKKK", "KKKKKK", "KKKKKK"],
      d=["KKKKKK", "rrrrrr", "KKKKKK", "KKKKKK", "KKKKKK", "KKKKKK"], fond="K")
icone("Objets", "Terminal", "terminal",
      {"M": ("m", "#232838"), "g": ("x", "#4ADE80")},
      g=["MMMMMM", "MgggMM", "MMMMMM", "MggMgM", "MMMMMM", "MgMMMM"])
icone("Objets", "Damier", "damier",
      {"K": ("m", "#2A2C31"), "W": ("m", "#F4F3EF")},
      t=["KKWWKK", "KKWWKK", "WWKKWW", "WWKKWW", "KKWWKK", "KKWWKK"],
      g=["WWKKWW", "WWKKWW", "KKWWKK", "KKWWKK", "WWKKWW", "WWKKWW"],
      d=["KKWWKK", "KKWWKK", "WWKKWW", "WWKKWW", "KKWWKK", "KKWWKK"])
icone("Objets", "Caisse en bois", "caisse",
      {"F": ("m", "#8A5A33"), "M": ("m", "#C68D57"), "m": ("m", "#C68D57", -0.08)},
      t=["FFFFFF", "FMMMMF", "FmmmmF", "FMMMMF", "FmmmmF", "FFFFFF"],
      g=["FFFFFF", "FMMMMF", "FmmmmF", "FMMMMF", "FmmmmF", "FFFFFF"],
      d=["FFFFFF", "FMMMMF", "FmmmmF", "FMMMMF", "FmmmmF", "FFFFFF"])

# Ajoutées le 23/11 : 10 icônes par groupe.
# ---- Serveurs : 5 pierres précieuses (même boîtier que les serveurs métal, un éclat sur le dessus).
METAUX.update({
    "diamant":   ["#06232B", "#0B3B46", "#115661", "#19737E", "#24929C", "#38B2B9", "#5ACFD2", "#8CE5E4", "#C3F5F2", "#F0FFFE"],
    "emeraude":  ["#041F0F", "#083718", "#0D5124", "#146C32", "#1C8A42", "#2AA856", "#47C470", "#7ADC97", "#B3F0C5", "#E7FDEE"],
    "rubis":     ["#1E0C0E", "#35171B", "#4F2229", "#6A3038", "#863F49", "#A1535E", "#BA7079", "#D399A1", "#E9C6CB", "#FAEFF1"],
    "amethyste": ["#17101F", "#291D37", "#3E2C51", "#553C6E", "#6D4E8A", "#8666A5", "#A084BD", "#BDA7D4", "#DBCFE9", "#F6F2FB"],
    "saphir":    ["#0D1221", "#182139", "#243154", "#33436F", "#41568A", "#546DA3", "#6F89BA", "#95AAD0", "#C1CFE6", "#ECF1FA"],
})
for metal, nom in [("diamant", "Serveur diamant"), ("emeraude", "Serveur émeraude"), ("rubis", "Serveur rubis"),
                   ("amethyste", "Serveur améthyste"), ("saphir", "Serveur saphir")]:
    icone("Serveurs", nom, "serveur-" + metal,
          {"M": ("metal", metal), "A": ("metal", metal, -1), "V": ("x", "#5ED68A"), "O": ("x", "#F2C14E"),
           "e": ("x", "#FFFFFF"), "E": ("metal", metal, 0.25)},
          t=["MMMMMM", "MMMMEM", "MMMEeM", "MMMMEM", "MMMMMM", "MMMMMM"],
          g=["MMMMMM", "MAAAAM", "MMMMMM", "MAAAAM", "MVMOMM", "MMMMMM"],
          d=["MMMMMM", "MMMMMM", "MAAAAM", "MMMMMM", "MMMMMM", "MMMMMM"])

# ---- Fête
icone("Fête", "Citrouille", "citrouille",
      {"M": ("m", "#F08A24"), "r": ("m", "#F08A24", -0.13), "V": ("m", "#4E8A2E"), "y": ("x", "#FFD34D")},
      t=["MMMMMM", "MMrrMM", "MrVVrM", "MrVVrM", "MMrrMM", "MMMMMM"],
      g=["MrMMrM"] * 6,
      d=["MrMMrM"] * 6)
icone("Fête", "Lanterne", "lanterne",
      {"R": ("m", "#D8322E"), "r": ("m", "#D8322E", -0.14), "l": ("v", "#FF9A5C", 0.08), "L": ("v", "#FFC27A", 0.12),
       "G": ("m", "#E9B949"), "k": ("m", "#5A1E14")},
      t=["GGGGGG", "GGGGGG", "GGkkGG", "GGkkGG", "GGGGGG", "GGGGGG"],
      g=["GGGGGG", "RrRRrR", "RrllrR", "RrLLrR", "RrRRrR", "GGGGGG"],
      d=["GGGGGG", "RrRRrR", "RrllrR", "RrLLrR", "RrRRrR", "GGGGGG"])

# ---- Gourmandises
icone("Gourmandises", "Pastèque", "pasteque",
      {"G": ("m", "#2F8F3A"), "g": ("m", "#2F8F3A", -0.14), "C": ("m", "#C9E7A6"), "R": ("m", "#EC4A55"), "s": NOIR},
      t=["CCCCCC", "CRRRRC", "CRsRRC", "CRRRsC", "CsRRRC", "CCCCCC"],
      g=["GgGGgG"] * 6,
      d=["GgGGgG"] * 6)
icone("Gourmandises", "Beignet", "beignet",
      {"P": ("m", "#F48FB1"), "D": ("m", "#D9A35F"), "h": ("m", "#D9A35F", -0.42), "y": ("x", "#FFE15A"), "b": ("x", "#5BB8F5"),
       "w": ("x", "#FFFFFF"), "o": ("vide",)},
      t=["PPyPPP", "PbPPwP", "PPhhPy", "PwhoPP", "PPPbPP", "PyPPwP"],
      g=["PPPPPP", "PDPPDP", "DDDDDD", "DDDDDD", "DDDDDD", "DDDDDD"],
      d=["PPPPPP", "DPPDPP", "DDDDDD", "DDDDDD", "DDDDDD", "DDDDDD"])
icone("Gourmandises", "Chocolat", "chocolat",
      {"M": ("m", "#6B3B22"), "c": ("m", "#6B3B22", -0.13), "P": ("m", "#7E3FB8"), "A": ("metal", "acier", 0.1)},
      t=["MMcMMc", "MMcMMc", "cccccc", "MMcMMc", "MMcMMc", "cccccc"],
      g=["MMcMMc", "cccccc", "AAAAAA", "PPPPPP", "PPPPPP", "PPPPPP"],
      d=["MMcMMc", "cccccc", "AAAAAA", "PPPPPP", "PPPPPP", "PPPPPP"])
icone("Gourmandises", "Gaufre", "gaufre",
      {"L": ("m", "#E6AA52"), "D": ("m", "#E6AA52", -0.3), "d": ("m", "#E6AA52", -0.14), "B": ("v", "#FFF0A8"),
       "S": ("m", "#B8661E", 0.05)},
      t=["LLLLLL", "LDLDLD", "LLBBSL", "LDBBDS", "LLSSLL", "LDLSLD"],
      g=["LLLSLL", "LdLSLd", "LLLLLL", "LdLdLd", "LLLLLL", "LdLdLd"],
      d=["LLLLLL", "dLdLdL", "LLLLLL", "dLdLdL", "LLLLLL", "dLdLdL"])

# ---- Objets
icone("Objets", "Livres", "livres",
      {"R": ("m", "#C8423B"), "B": ("m", "#3E6FC9"), "G": ("m", "#3E9A5C"), "y": ("v", "#F2C14E"), "c": ("m", "#F3E9D2")},
      t=["RRRRRR", "RRRRRR", "RyyyyR", "RRRRRR", "RRRRRR", "RRRRRR"],
      g=["RRRRRR", "RyRRyR", "BBBBBB", "ByBByB", "GGGGGG", "GyGGyG"],
      d=["Rccccc", "RRRRRR", "Bccccc", "BBBBBB", "Gccccc", "GGGGGG"])

# ---- Nature
icone("Nature", "Bûche", "buche",
      {"B": ("m", "#7E5232"), "b": ("m", "#7E5232", -0.13), "L": ("m", "#E2BF85"), "D": ("m", "#B98A52")},
      t=["BBBBBB", "BLLLLB", "BLDDLB", "BLDDLB", "BLLLLB", "BBBBBB"],
      g=["BbBBbB", "BbBBBb", "bBBbBB", "BBbBbB", "BbBBbB", "bBBbBB"],
      d=["BBbBBb", "bBBbBB", "BbBBbB", "BBbBBb", "bBBbBB", "BbBBbB"])
icone("Nature", "Champignon", "champignon",
      {"R": ("m", "#D9343A"), "w": ("x", "#FFFFFF"), "S": ("m", "#F1E4C8"), "s": ("m", "#F1E4C8", -0.12)},
      t=["RRRRRR", "RwRRwR", "RRRRRR", "RRwRRR", "RRRRwR", "RwRRRR"],
      g=["RRwRRR", "RRRRwR", "sssssss"[:6], "SSSSSS", "SSSSSS", "SSSSSS"],
      d=["RwRRRR", "RRRwRR", "ssssss", "SSSSSS", "SSSSSS", "SSSSSS"])
icone("Nature", "Cactus", "cactus",
      {"V": ("m", "#3F9A4A"), "v": ("m", "#3F9A4A", -0.13), "w": ("x", "#F4F6E8"), "p": ("v", "#F27EB0"),
       "y": ("v", "#F6D23C")},
      t=["VVVVVV", "VvVVvV", "VVppVV", "VVpyVV", "VvVVvV", "VVVVVV"],
      g=["VvVVvV", "wvVVvV", "VvVwvV", "VvVVvw", "VvwVvV", "VvVVvV"],
      d=["VvVVvV", "VvwVvV", "wvVVvV", "VvVVvw", "VvVwvV", "VvVVvV"])
icone("Nature", "Cerisier", "cerisier",
      {"P": ("m", "#F6BCD2"), "p": ("m", "#E890B3"), "w": ("x", "#FFF4F8")},
      t=["PPpPPp", "PwpPPp", "ppPPpp", "PPpPwp", "PPpPPp", "ppPPpp"],
      g=["PPpPPp", "PwpPPp", "ppPPpp", "PPpPwp", "PPpPPp", "ppPPpp"],
      d=["pPPpPP", "pPwpPP", "ppPPpp", "pwPPpP", "pPPpPP", "ppPPpp"])
icone("Nature", "Plage", "plage",
      {"A": ("m", "#2FB5D9"), "a": ("m", "#2FB5D9", 0.1), "w": ("x", "#F2FBFF"), "S": ("m", "#EAD39A"), "s": ("m", "#EAD39A", -0.1)},
      t=["AAAAAA", "AAaAAA", "AAAAaA", "wwwwww", "SSSSSS", "SSsSSS"],
      g=["SSSSSS", "SsSSSS", "SSSSsS", "SSSSSS", "SsSSSS", "SSSSSS"],
      d=["SSwAAA", "SSSwAA", "SSSSwA", "SSsSSS", "SSSSSS", "SsSSSS"])

CADEAU_T = DESSUS_CADEAU
CADEAU_F = FACE_CADEAU
icone("Fête", "Pull de Noël", "pull",
      {"R": ("m", "#C23A3A"), "W": ("m", "#F4F1EA"), "G": ("m", "#2E8048")},
      t=["RRRRRR"] * 6,
      g=["RRRRRR", "RRRRRR", "WRWRWR", "RWRWRW", "RRRRRR", "RRRRRR"],
      d=["RRRRRR", "RRRRRR", "RWRWRW", "WRWRWR", "RRRRRR", "RRRRRR"])
icone("Fête", "Cadeau violet", "cadeau-violet",
      {"B": ("m", "#8E5BD9"), "R": ("metal", "acier", 0.15), "K": ("metal", "acier", 0.3)},
      t=CADEAU_T, g=CADEAU_F, d=CADEAU_F, fond="B")
icone("Fête", "Bougie", "bougie",
      {"R": ("m", "#C8323A"), "W": ("m", "#F6EFE2"), "k": NOIR, "f": ("x", "#FFC93C"), "F": ("x", "#FF8A3D")},
      t=["WWWWWW", "WWWWWW", "WWfFWW", "WWFkWW", "WWWWWW", "WWWWWW"],
      g=["WWWWWW", "RWRRWR", "RWRRRR", "RRRRRR", "RRRRRR", "RRRRRR"],
      d=["WWWWWW", "RRWRRW", "RRWRRR", "RRRRRR", "RRRRRR", "RRRRRR"])
icone("Gourmandises", "Cookie", "cookie",
      {"M": ("m", "#C98A4B"), "k": ("m", "#4A2A18")},
      t=["MMMMMM", "MkMMMM", "MMMMkM", "MMkMMM", "kMMMMM", "MMMkMM"],
      g=["MMMMMM", "MMkMMM", "MMMMMk", "MkMMMM", "MMMkMM", "MMMMMM"],
      d=["MMMMMM", "MkMMMM", "MMMMkM", "MMkMMM", "MMMMMM", "kMMMkM"])
icone("Gourmandises", "Burger", "burger",
      {"B": ("m", "#D99A4E"), "w": ("x", "#FFF6E0"), "L": ("m", "#5DB846"), "C": ("m", "#F5C33B"), "P": ("m", "#5A3420")},
      t=["BBBBBB", "BwBBwB", "BBBwBB", "BwBBBB", "BBBBwB", "BBBBBB"],
      g=["BBBBBB", "BBBBBB", "LLLLLL", "CCCCCC", "PPPPPP", "BBBBBB"],
      d=["BBBBBB", "BBBBBB", "LLLLLL", "CCCCCC", "PPPPPP", "BBBBBB"])
icone("Objets", "Télé rétro", "tele",
      {"M": ("m", "#D8CFBF"), "S": ("m", "#3D4A5C"), "s": ("m", "#6D84A3"), "k": NOIR, "l": ("m", "#D8CFBF", -0.15)},
      t=["MMMMMM"] * 6,
      g=["MMMMMM", "MSSSSM", "MSsSSM", "MSSSSM", "MSSSSM", "MMMMMM"],
      d=["MMMMMM", "MlllkM", "MMMMMM", "MlllkM", "MMMMMM", "MMMMMM"])
icone("Objets", "Coffre au trésor", "tresor",
      {"W": ("m", "#7A4A2B"), "w": ("m", "#7A4A2B", -0.12), "G": ("m", "#E2B649"), "y": ("m", "#E2B649", 0.12), "k": NOIR},
      t=["GGGGGG", "GWWWWG", "GwwwwG", "GWWWWG", "GwwwwG", "GGGGGG"],
      g=["GWWWWG", "GwwwwG", "GGGGGG", "GWyyWG", "GwykwG", "GWWWWG"],
      d=["GWWWWG", "GwwwwG", "GGGGGG", "GWWWWG", "GwwwwG", "GWWWWG"])
icone("Objets", "Disquette", "disquette",
      {"B": ("m", "#3563B8"), "A": ("metal", "acier", 0.15), "k": ("m", "#3563B8", -0.3), "W": ("m", "#F4F1EA"), "l": ("m", "#F4F1EA", -0.2)},
      t=["BBBBBB"] * 6,
      g=["BAAkAB", "BAAkAB", "BBBBBB", "BWWWWB", "BWllWB", "BWWWWB"],
      d=["BBBBBB"] * 6)
icone("Nature", "Ruche", "ruche",
      {"H": ("m", "#E3AA3A"), "h": ("m", "#E3AA3A", -0.2), "k": ("m", "#3A2412"), "y": ("x", "#FFD23F"), "b": ("x", "#1E2127"),
       "D": ("v", "#F7B733", 0.1)},
      t=["hhhhhh", "hHHHHh", "hHhhHh", "hHhhHh", "hHHHHh", "hhhhhh"],
      g=["HHDHHH", "hhDhhh", "HHDHHH", "hhhhhh", "HHkkHH", "hhkkhh"],
      d=["HHHHDH", "hhhhDh", "HHHHHH", "hhhhhh", "HHHHHH", "hhhhhh"])


# Numéros 0 à 9 (21/11, style D « ardoise bleutée ») : cube mat, chiffre blanc 3 × 5 sur la façade gauche,
# colonnes 2 à 4, collé en haut (rangées 0 à 4) : il ne touche jamais le bas, sinon il disparaît sur fond blanc.
CHIFFRES = {
    "0": ["###", "#.#", "#.#", "#.#", "###"],
    "1": [".#.", "##.", ".#.", ".#.", "###"],
    "2": ["###", "..#", "###", "#..", "###"],
    "3": ["###", "..#", ".##", "..#", "###"],
    "4": ["#.#", "#.#", "###", "..#", "..#"],
    "5": ["###", "#..", "###", "..#", "###"],
    "6": ["###", "#..", "###", "#.#", "###"],
    "7": ["###", "..#", "..#", ".#.", ".#."],
    "8": ["###", "#.#", "###", "#.#", "###"],
    "9": ["###", "#.#", "###", "..#", "###"],
}


def face_chiffre(c, x0=2):
    g = [list("M" * N) for _ in range(N)]
    for r, ligne in enumerate(CHIFFRES[c]):
        for i, ch in enumerate(ligne):
            if ch == "#":
                g[r][x0 + i] = "x"
    return ["".join(l) for l in g]


# Cadre plus foncé sur le dessus et la face droite (23/11) : un numéro ne ressemble plus à un cube de couleur.
CADRE = ["DDDDDD", "DMMMMD", "DMMMMD", "DMMMMD", "DMMMMD", "DDDDDD"]
for c in "0123456789":
    icone("Numéros", f"Numéro {c}", f"numero-{c}", {"M": ("m", "#66789A"), "D": ("m", "#66789A", -0.1), "x": ("x", "#FFFFFF")},
          t=CADRE, g=face_chiffre(c), d=CADRE)


# L'ordre de la fenêtre « Icône… » (le même que src/theme/icones.ts et ICONES de serveurs.rs).
RANG = [
    "graphite", "bleu", "indigo", "violet", "rose", "rouge", "orange", "jaune", "vert", "menthe",
    "numero-0", "numero-1", "numero-2", "numero-3", "numero-4", "numero-5", "numero-6", "numero-7", "numero-8", "numero-9",
    "serveur-acier", "serveur-titane", "serveur-noir", "serveur-dore", "serveur-orrose",
    "serveur-rubis", "serveur-amethyste", "serveur-saphir", "serveur-diamant", "serveur-emeraude",
    "cadeau-rouge", "cadeau-bleu", "cadeau-violet", "cadeau-vert", "pull", "gateau", "gateau-chocolat", "bougie", "citrouille", "lanterne",
    "burger", "bento", "fromage", "pain", "pasteque", "cookie", "gaufre", "beignet", "chocolat", "guimauve",
    "carton", "caisse", "tresor", "livres", "de", "damier", "tele", "camera", "disquette", "terminal",
    "herbe", "buche", "champignon", "cactus", "cerisier", "ruche", "grenouille", "plage", "aquarium", "hiver",
]
assert sorted(RANG) == sorted(ic["cle"] for ic in ICONES), "RANG et les icônes ne correspondent pas"
ICONES.sort(key=lambda ic: RANG.index(ic["cle"]))
PROPOSITIONS = []


# ---------------------------------------------------------------- planche

def police(t, gras=False):
    for f in (f"/usr/share/fonts/truetype/dejavu/DejaVuSans{'-Bold' if gras else ''}.ttf",):
        try:
            return ImageFont.truetype(f, t)
        except Exception:
            pass
    return ImageFont.load_default()


def planche(chemin, icones=None):
    icones = icones or ICONES
    colonnes = 6
    groupes = []
    for ic in icones:
        if not groupes or groupes[-1][0] != ic["groupe"]:
            groupes.append((ic["groupe"], []))
        groupes[-1][1].append(ic)
    case_w, case_h, marge = 190, 230, 40
    hauteur = 40
    for _, liste in groupes:
        hauteur += 60 + math.ceil(len(liste) / colonnes) * case_h
    largeur = 2 * marge + colonnes * case_w
    fonds = [("#F6F2EC", "#2B2427", "#7A7072"), ("#17161E", "#ECEBF4", "#9A97A8")]
    feuille = Image.new("RGB", (largeur * 2, hauteur), fonds[0][0])
    d = ImageDraw.Draw(feuille)
    d.rectangle([largeur, 0, largeur * 2, hauteur], fill=fonds[1][0])
    f_titre, f_nom = police(26, True), police(17)
    for k, (fond, texte, gris) in enumerate(fonds):
        x0 = k * largeur
        y = 40
        for groupe, liste in groupes:
            d.text((x0 + marge, y), groupe, fill=texte, font=f_titre)
            y += 60
            for n, ic in enumerate(liste):
                cx = x0 + marge + (n % colonnes) * case_w
                cy = y + (n // colonnes) * case_h
                grand = png(ic, 140)
                feuille.paste(grand, (cx + 10, cy), grand)
                petit = png(ic, 28)
                feuille.paste(petit, (cx + 150, cy + 112), petit)
                d.text((cx + 10, cy + 150), ic["nom"], fill=texte, font=f_nom)
            y += math.ceil(len(liste) / colonnes) * case_h
    feuille.save(chemin)


if __name__ == "__main__":
    os.makedirs("sortie/png", exist_ok=True)
    planche("sortie/planche.png")

    os.makedirs("sortie/app", exist_ok=True)
    os.makedirs("sortie/jeu", exist_ok=True)
    for ic in ICONES:
        png(ic, 256).save(f"sortie/png/{ic['cle']}.png")
        # Sans marge : le cube touche le haut et le bas, comme avant avec les SVG.
        png(ic, 192, marge=0).save(f"sortie/app/{ic['cle']}.png", optimize=True)
        png(ic, 64, marge=0).save(f"sortie/jeu/{ic['cle']}.png", optimize=True)
    import json
    json.dump([dict(cle=ic["cle"], nom=ic["nom"], groupe=ic["groupe"]) for ic in ICONES], open("sortie/liste.json", "w"), ensure_ascii=False, indent=1)
    print(len(ICONES), "icônes")
