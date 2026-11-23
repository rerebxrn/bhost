# Confidentialité

BHOST n’a pas de compte utilisateur, pas de publicité, pas de mesure d’audience.
Il n’envoie rien sur toi ni sur tes serveurs pour les analyser.

Il se connecte à Internet seulement pour les choses suivantes.

## Automatiquement

| Quand | Vers | Ce qui part |
| --- | --- | --- |
| À chaque lancement | GitHub (`github.com`) | Rien d’autre que ton adresse IP : BHOST lit la dernière version publiée. |

## Quand tu le demandes

| Action | Vers | Ce qui part |
| --- | --- | --- |
| Créer un serveur, changer de version | Mojang, PaperMC, Purpur, Fabric, Forge, NeoForge, MohistMC, SpigotMC, Adoptium (Java) | Le nom et la version du logiciel à télécharger. |
| Plugins, mods, datapacks, modpacks | Modrinth, GeyserMC | Ta recherche, ou l’empreinte d’un fichier pour le reconnaître. |
| Joueurs (têtes, liste blanche, bannir) | Mojang | Les pseudos concernés. |
| « Pourquoi ça lag ? » | spark (`spark.lucko.me`) | Le rapport de performance du serveur (publié en ligne par spark, lien donné à toi seul). |
| « Ouvert à tes amis » par le relais | Relais BHOST (`relais.bhost.app`) | Les connexions des joueurs vers ton serveur passent par le relais : il voit leur adresse IP. Il la garde en mémoire au plus 1 heure, pour limiter les abus, et ne l’écrit nulle part. |
| Lier une machine dédiée | Relais BHOST | Une empreinte du code de la machine (jamais le code lui-même). Le pilotage est chiffré de bout en bout. |

Chacune de ces fonctions se désactive : il suffit de ne pas l’utiliser, ou de couper « Ouvert à tes amis »
dans les paramètres du serveur.

## Contact

Une question : ouvre une issue sur [GitHub](https://github.com/rerebxrn/bhost/issues).
