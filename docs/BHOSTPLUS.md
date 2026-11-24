# BHOST+

L’abonnement : thème Ambre, 40 icônes en plus, relais prioritaire, versions en avance. 3 € par mois ou 30 € par an,
une clé pour 3 PC. Vendu par Polar (`polar.sh`), qui envoie la clé par email.
Code : `src-tauri/src/plus.rs` et `src/plus/` (appli), `plus/` (service sur le serveur du relais), `relais/` (priorité).

Si `ORGANISATION` est vide dans `plus.rs`, BHOST+ est caché et rien n’est fermé.

## Polar (organisation « rebxrn »)

Déjà fait (23/11) :
- Produits « BHOST+ mensuel » (3 €/mois) et « BHOST+ annuel » (30 €/an), avantage « Clé BHOST+ (3 PC) » :
  clé de licence, préfixe `BHOST`, 3 activations, le client peut retirer un PC depuis l’espace client.
- Dans `src-tauri/src/plus.rs` : `ORGANISATION`, les deux liens de paiement et l’espace client
  (`https://polar.sh/rebxrn/portal`).

Reste à faire :
1. Polar › Settings › Developers › New token (organisation), droits `license_keys:read` et `subscriptions:read`.
   Ce jeton est **secret** : il ne va que sur le serveur (demandé par `installer-relais.sh`, gardé dans
   `/opt/bhost-relais/plus.env`), jamais dans Git ni dans l’appli.

L’appli n’a aucun secret : elle utilise les adresses publiques de Polar et `plus.bhost.app`.

## Le serveur (relais + BHOST+)

`relais/deploiement/installer-relais.sh` installe le relais, le service BHOST+ et Caddy (HTTPS de `plus.bhost.app`) :
```
scp relais\deploiement\installer-relais.sh root@62.83.119.217:/root/
ssh -t root@62.83.119.217 bash /root/installer-relais.sh
```
À faire **avant** de publier une version de BHOST avec BHOST+ (l’ancien relais coupe les BHOST qui envoient le jeton).

- `plus.bhost.app/etat` : les PC de la clé, le retard de paiement, et un jeton signé pour le relais (2 jours).
- Relais prioritaire : au-delà de 80 % des places (`--joueurs-max 400`), seuls les serveurs BHOST+ font encore entrer
  leurs joueurs. Les autres voient « Le relais BHOST est plein ».

## Comment ça marche

- Réglages › BHOST+ › Clé › Activer : la clé est activée sur ce PC (une place sur 3).
- Revérifiée au lancement puis toutes les 6 heures (par `plus.bhost.app`, sinon directement chez Polar).
  Sans Internet : 7 jours, prévenu à partir du 3e, puis « en pause » jusqu’à la prochaine connexion.
- Paiement refusé : « Paiement en retard » (tout reste ouvert pendant que Polar réessaie), bouton « Mettre à jour ».
- 3 PC sur 3 : Réglages montre les PC, on en libère un (« Libérer… ») et celui-ci s’active.
- Clé retirée ou abonnement arrêté : « expiré ». Le thème repasse en Azur, les serveurs avec une icône
  payante reprennent celle de leur logiciel.
- « Retirer de ce PC » libère la place ; « Libérer… » libère celle d’un autre PC.

## Essayer sans payer

En développement (`npm run tauri dev`), ces clés marchent sans Polar :

| Clé | Résultat |
| --- | --- |
| `TEST-ACTIF` | Abonné (avec un 2e PC à libérer). |
| `TEST-RETARD` | Paiement en retard. |
| `TEST-PLEIN` | 3 PC sur 3 : en libérer un, puis ce PC s’active. |
| `TEST-EXPIRE` | Abonné, puis expiré 10 secondes après. |
| `TEST-HORSLIGNE` | Abonné, 4 jours sans Internet (bandeau). |
| `TEST-PAUSE` | En pause (8 jours sans Internet). |
| `TEST-…` autre | Clé inconnue. |

Ces clés n’existent pas dans la version publiée.
