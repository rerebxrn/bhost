# Service BHOST+

Sur le serveur du relais, derrière Caddy : `https://plus.bhost.app/etat`.
L'appli y demande ce que Polar ne donne qu'avec la clé d'accès de l'organisation :
les PC où la clé est activée, le paiement en retard, et le jeton du relais prioritaire
(signé ici avec `plus.cle`, vérifié par le relais avec `plus.cle.pub`).

```
POLAR_JETON=... POLAR_ORGANISATION=... BHOST_PLUS_CLE=plus.cle cargo run
```

Installation : `relais/deploiement/installer-relais.sh` (relais + BHOST+ + Caddy). Essais : `cargo test`.
