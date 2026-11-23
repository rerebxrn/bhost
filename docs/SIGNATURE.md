# Code signing policy

BHOST publie ses fichiers Windows (`BHOST-Setup.exe` et l’installeur de l’appli) depuis ce dépôt public,
[`rerebxrn/bhost`](https://github.com/rerebxrn/bhost).

## Statut

Signature Windows (Authenticode) : **demande en préparation** auprès de la SignPath Foundation.
Une fois acceptée, les versions seront signées ainsi :

> Free code signing provided by [SignPath.io](https://signpath.io), certificate by [SignPath Foundation](https://signpath.org).

En attendant, chaque version est déjà signée pour les mises à jour automatiques (clé du projet, vérifiée par BHOST)
et l’agent Linux des machines dédiées est signé avec la même clé.

## Ce qui est signé

- `BHOST-Setup.exe`
- l’installeur de l’appli (`BHOST_x.y.z_x64-setup.exe`)

Ces fichiers sont fabriqués uniquement par GitHub Actions (`.github/workflows/publier.yml`), à partir du code
de ce dépôt. Rien n’est compilé ni signé à la main.

## Équipe

| Rôle | Membres |
| --- | --- |
| Auteurs (committers) | [Thomas (@rerebxrn)](https://github.com/rerebxrn) |
| Relecteurs (reviewers) | [Thomas (@rerebxrn)](https://github.com/rerebxrn) |
| Approbateurs (approvers) | [Thomas (@rerebxrn)](https://github.com/rerebxrn) |

Les contributions extérieures (pull requests) sont relues avant d’être acceptées.
Tous les membres utilisent l’authentification à deux facteurs sur GitHub et SignPath.

## Confidentialité

Voir [`CONFIDENTIALITE.md`](CONFIDENTIALITE.md).
