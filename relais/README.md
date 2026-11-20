# Relais BHOST

Quand la box d'un joueur ne s'ouvre pas (UPnP coupé, CGNAT, 5G), BHOST se connecte à ce relais
et ses amis rejoignent `nom-xxxx.bhost.app`.

- Port **25565** : les joueurs (le DNS `*.bhost.app` pointe ici).
- Port **4443** : BHOST (une connexion par serveur en ligne, puis une par joueur).
- `noms.json` : quel nom appartient à quelle clé (un nom pas vu depuis 180 jours est libéré).

```
cargo build --release
./target/release/bhost-relais --domaine bhost.app --joueurs 0.0.0.0:25565 --agents 0.0.0.0:4443 --registre noms.json
```

Essais : `cargo test` (ici), et `cargo test --lib tunnel` dans `src-tauri` (BHOST + relais de bout en bout).
