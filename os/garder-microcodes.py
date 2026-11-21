#!/usr/bin/env python3
"""Ne garde que les microcodes (firmware) utiles à un serveur BHOST OS : Wi-Fi, réseau, cartes graphiques courantes.
Usage : garder-microcodes.py <dossier firmware> <liste des microcodes demandés par les pilotes (modinfo -F firmware)>"""
import os, re, sys

base, liste = sys.argv[1], sys.argv[2]
voulus = {l.strip() for l in open(liste) if l.strip()}
# Familles toujours gardées entières (les pilotes y cherchent des fichiers qu'ils ne déclarent pas tous).
TOUJOURS = ('ath10k/', 'ath11k/', 'ath9k_htc/', 'rtw88/', 'rtw89/', 'rtlwifi/', 'rtl_nic/', 'i915/', 'radeon/',
            'mediatek/mt7', 'brcm/brcmfmac', 'regulatory')
# Jamais : cartes serveur, son, caméras, téléphones, puces très rares.
JAMAIS = ('nvidia/', 'mellanox/', 'mrvl/', 'qed/', 'netronome/', 'dpaa2/', 'cxgb4/', 'bnx2x/', 'liquidio/', 'qcom/',
          'ti-connectivity/', 'cirrus/', 'xe/', 'intel/vsc/', 'intel/ipu/', 'intel/vpu/', 'intel/ish/', 'amdgpu/',
          'ath12k/', 'qca/', 'mediatek/mt8', 'intel/ibt-', 'rtl_bt/', 'brcm/BCM', 'cypress/', 'ti/', 'intel/sof',
          'intel/avs', 'intel/catpt', 'intel/IntcSST', 'intel/fw_sst', 'intel/dsp', 'intel-ucode/', 'amd-ucode/')

fichiers = []
for r, _, f in os.walk(base):
    for x in f:
        fichiers.append(os.path.relpath(os.path.join(r, x), base))
sans_zst = lambda p: p[:-4] if p.endswith('.zst') else p
index = {sans_zst(p): p for p in fichiers}

garder = set()
for v in voulus:
    if v.startswith(JAMAIS):
        continue
    if v in index:
        garder.add(index[v])
        continue
    # iwlwifi-xxx-NN.ucode : le pilote essaie NN, puis les versions en dessous ; on garde la plus haute présente.
    m = re.match(r'(.*-)(\d+)(\.ucode)$', v)
    if m:
        rx = re.compile(re.escape(m.group(1)) + r'(\d+)' + re.escape(m.group(3)) + '$')
        c = [(int(rx.match(k).group(1)), k) for k in index if rx.match(k) and int(rx.match(k).group(1)) <= int(m.group(2))]
        if c:
            garder.add(index[max(c)[1]])
for k, p in index.items():
    if k.startswith(TOUJOURS) and not k.startswith(JAMAIS):
        garder.add(p)
# Les liens gardés emmènent leur cible.
for g in list(garder):
    p = os.path.join(base, g)
    while os.path.islink(p):
        p = os.path.realpath(p)
        garder.add(os.path.relpath(p, base))

n = 0
for p in fichiers:
    if p not in garder:
        os.remove(os.path.join(base, p))
        n += 1
print(f'microcodes : {len(garder)} gardés, {n} retirés')
