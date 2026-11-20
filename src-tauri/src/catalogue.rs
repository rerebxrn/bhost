// Onglet Plugins / Mods / Datapacks : les commandes de l'interface.
// Les fichiers et la lecture du catalogue en ligne sont dans extensions.rs (testés sans Tauri).

use crate::extensions::{self as ext, Cache, Fichier, Genre, Liens, Panne, Projet, Tri, VersionMr};
use crate::serveurs::Serveur;
use crate::{Etat, a_part, en_ligne, reseau, serveur_de};
use serde::Serialize;
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

/// Ce que l'onglet garde en mémoire.
pub struct EtatCatalogue {
    cache: Mutex<Cache>,
    liens: Mutex<Liens>,
    fichier_liens: PathBuf,
    /// Les serveurs allumés dont les extensions ont changé : il faut les redémarrer.
    a_redemarrer: Mutex<HashSet<String>>,
    /// Les plugins qui n'ont pas démarré, par serveur (lus dans la console depuis le dernier démarrage).
    pannes: Mutex<HashMap<String, Pannes>>,
    /// Les versions qui demandent un Java plus récent que celui de leur serveur :
    /// « id de version|java du serveur » → Java demandé (gardé dans trop_recentes.json).
    trop_recentes: Mutex<HashMap<String, u32>>,
    fichier_trop_recentes: PathBuf,
    /// Les datapacks retirés : « id du serveur|fichier » → projet du catalogue (gardé dans retires.json),
    /// pour pouvoir les remettre si le monde ne se charge plus sans eux.
    retires: Mutex<HashMap<String, String>>,
    fichier_retires: PathBuf,
}

/// Les pannes d'un serveur : (fichier ou nom en minuscules, raison, en jeu = il a démarré mais plante).
#[derive(Default)]
struct Pannes {
    liste: Vec<(String, Option<String>, bool)>,
    /// Combien de lignes depuis la dernière panne : une raison ne compte que juste après.
    depuis: u32,
    /// « Missing data pack file/X.zip » : les datapacks que le monde attend et qui ne sont plus là.
    manquants: Vec<String>,
    /// « Failed to load datapacks » : le monde ne se charge plus.
    bloque: bool,
}

impl EtatCatalogue {
    /// `dossier` : %APPDATA%\BHOST (les liens « installé pour » vont dans extensions.json).
    pub fn nouveau(dossier: &Path) -> EtatCatalogue {
        let fichier_liens = dossier.join("extensions.json");
        let fichier_trop_recentes = dossier.join("trop_recentes.json");
        let trop_recentes = std::fs::read(&fichier_trop_recentes).ok().and_then(|o| serde_json::from_slice(&o).ok()).unwrap_or_default();
        let fichier_retires = dossier.join("retires.json");
        let retires = std::fs::read(&fichier_retires).ok().and_then(|o| serde_json::from_slice(&o).ok()).unwrap_or_default();
        EtatCatalogue {
            cache: Mutex::default(),
            liens: Mutex::new(Liens::charger(&fichier_liens)),
            fichier_liens,
            a_redemarrer: Mutex::default(),
            pannes: Mutex::default(),
            trop_recentes: Mutex::new(trop_recentes),
            fichier_trop_recentes,
            retires: Mutex::new(retires),
            fichier_retires,
        }
    }
}

/// Le serveur vient de démarrer : ses extensions sont à jour, et les pannes d'avant sont oubliées.
pub fn demarre(app: &AppHandle, id: &str) {
    let etat = app.state::<Etat>();
    etat.catalogue.a_redemarrer.lock().unwrap().remove(id);
    let bloque = etat.catalogue.pannes.lock().unwrap().remove(id).is_some_and(|p| p.bloque);
    // Le message « Le monde ne se charge plus » disparaît (il reviendra si ça replante).
    if bloque {
        let _ = app.emit("extensions", id);
    }
}

/// Une ligne de la console : un plugin qui n'a pas démarré ? L'onglet est prévenu (event « extensions »).
pub fn suivre_pannes(app: &AppHandle, id: &str, ligne: &str) {
    let etat = app.state::<Etat>();
    let mut toutes = etat.catalogue.pannes.lock().unwrap();
    let p = toutes.entry(id.to_string()).or_default();
    p.depuis = p.depuis.saturating_add(1);
    let changement = match ext::lire_panne(ligne) {
        Some(Panne::Fichier(cle) | Panne::Nom(cle)) => {
            if p.liste.len() < 100 && !p.liste.iter().any(|(c, ..)| *c == cle) {
                p.liste.push((cle, None, false));
            }
            p.depuis = 0;
            true
        }
        // Il plante en jeu (souvent plusieurs fois) : une seule ligne, la dernière en date pour la raison.
        Some(Panne::Plante(cle)) => {
            p.depuis = 0;
            match p.liste.iter().position(|(c, ..)| *c == cle) {
                Some(i) => {
                    let deja = p.liste.remove(i);
                    let nouveau = deja.1.is_none();
                    p.liste.push(deja);
                    nouveau
                }
                None if p.liste.len() < 100 => {
                    p.liste.push((cle, None, true));
                    true
                }
                None => false,
            }
        }
        Some(Panne::PackManquant(nom)) => {
            if p.manquants.len() < 20 && !p.manquants.contains(&nom) {
                p.manquants.push(nom);
            }
            p.bloque
        }
        Some(Panne::MondeBloque) => {
            p.bloque = true;
            !p.manquants.is_empty()
        }
        // La raison suit de près la panne (quelques lignes de détails).
        Some(Panne::Raison(r)) if p.depuis <= 12 => match p.liste.last_mut() {
            Some((_, raison @ None, _)) => {
                *raison = Some(r);
                true
            }
            _ => false,
        },
        _ => false,
    };
    drop(toutes);
    if changement {
        let _ = app.emit("extensions", id);
    }
}

/// Un serveur retiré de BHOST : on oublie ses liens.
pub fn oublier(app: &AppHandle, id: &str) {
    let etat = app.state::<Etat>();
    let mut liens = etat.catalogue.liens.lock().unwrap();
    liens.oublier(id);
    liens.enregistrer(&etat.catalogue.fichier_liens);
}

/// Une extension installée, pour l'interface.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Extension {
    /// Le nom du fichier sur le disque (« WorldEdit.jar » ou « WorldEdit.jar.disabled »).
    fichier: String,
    nom: String,
    version: Option<String>,
    description: Option<String>,
    couleur: Option<u32>,
    actif: bool,
    /// Un datapack en dossier : on ne peut que le supprimer.
    dossier: bool,
    /// La version plus récente qui existe, s'il y en a une.
    maj: Option<String>,
    /// « Installé pour WorldEdit ».
    pour: Option<String>,
    /// « Pas prévu pour 1.21.4. »
    alerte: Option<String>,
    /// L'identifiant dans le catalogue, si le fichier y est connu.
    projet: Option<String>,
    /// L'icône du projet dans le catalogue.
    icone: Option<String>,
    /// « Ne démarre pas. Il lui faut Java 25. » (vu dans la console au dernier démarrage).
    panne: Option<String>,
    /// La panne arrive en jeu (le plugin a démarré, puis plante) : « En jeu » dans la fiche.
    panne_en_jeu: bool,
    /// « En double avec WorldEdit-7.3.jar. » ou « Ne va pas avec FastAsyncWorldEdit. »
    conflit: Option<String>,
    /// Un datapack qui a façonné le terrain du monde (le monde a été lancé avec lui) : le retirer peut le casser.
    monde: bool,
}

/// Un datapack que le monde attend et qui n'est plus là (le serveur ne démarre plus).
#[derive(Serialize, Clone)]
pub struct Manquant {
    fichier: String,
    /// Le nom à montrer (« Terralith »).
    nom: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VueExtensions {
    genre: &'static str,
    /// Les genres possibles sur ce serveur (« plugins », « datapacks »).
    genres: Vec<&'static str>,
    /// Combien de fichiers dans chaque genre.
    nombres: HashMap<&'static str, usize>,
    /// Pour les datapacks : le monde où ils vont (« world »).
    monde: Option<String>,
    liste: Vec<Extension>,
    /// Le serveur tourne et ses extensions ont changé.
    redemarrer: bool,
    /// Le catalogue n'a pas répondu : les noms viennent des fichiers eux-mêmes.
    hors_ligne: bool,
    /// Le monde ne se charge plus : il lui manque ces datapacks.
    manquants: Vec<Manquant>,
}

struct Contexte {
    serveur: Serveur,
    genre: Genre,
    /// Tous les genres de ce serveur (le principal, puis les datapacks depuis la 1.13).
    genres: Vec<Genre>,
    dossier: PathBuf,
    /// La clé du cache pour ce serveur et ce genre (« Paper:datapacks »).
    cle: String,
    /// Des « loaders » imposés (ViaVersion n'existe qu'en « paper » dans le catalogue, il marche aussi sur Spigot).
    loaders: Option<Vec<&'static str>>,
}

/// `genre` : « plugins », « mods » ou « datapacks » (absent : le genre principal du serveur).
fn contexte(app: &AppHandle, id: &str, genre: Option<&str>) -> Result<Contexte, String> {
    let serveur = serveur_de(&app.state::<Etat>(), id)?;
    let genres = ext::genres(&serveur.logiciel, &serveur.version);
    let principal = *genres.first().ok_or("Bientôt pour ce logiciel.")?;
    let genre = match genre {
        None => principal,
        Some(g) => Genre::depuis_id(g).filter(|g| genres.contains(g)).ok_or("Pas possible sur ce serveur.")?,
    };
    let dossier = ext::dossier(Path::new(&serveur.dossier), genre);
    let cle = format!("{}:{}", serveur.logiciel, genre.id());
    Ok(Contexte { serveur, genre, genres, dossier, cle, loaders: None })
}

/// Les loaders du catalogue pour ce serveur et ce genre.
fn loaders_de(c: &Contexte) -> Vec<&'static str> {
    if let Some(l) = &c.loaders {
        return l.clone();
    }
    ext::loaders_genre(c.genre, &c.serveur.logiciel, &c.serveur.jar)
}

fn agent(app: &AppHandle) -> ureq::Agent {
    app.state::<Etat>().reseau.clone()
}

/// Demande au catalogue ce qu'il sait des fichiers pas encore en mémoire (3 demandes au plus).
fn identifier(app: &AppHandle, c: &Contexte, empreintes: &[String]) -> Result<(), String> {
    let etat = app.state::<Etat>();
    let (cle, version) = (c.cle.as_str(), c.serveur.version.as_str());
    let manquants: Vec<String> = {
        let cache = etat.catalogue.cache.lock().unwrap();
        empreintes.iter().filter(|h| cache.lire(h, cle, version).is_none()).cloned().collect()
    };
    if manquants.is_empty() {
        return Ok(());
    }
    let agent = agent(app);
    let reponse = reseau::envoyer_json(&agent, &format!("{}/version_files", ext::API), &json!({ "hashes": manquants, "algorithm": "sha1" }))?;
    let versions = ext::lire_par_empreinte(&reponse);
    // Les projets (nom, description, couleur) des fichiers reconnus.
    let ids: Vec<String> = versions.values().map(|v| v.projet.clone()).filter(|p| ext::id_valide(p)).collect::<HashSet<_>>().into_iter().collect();
    let mut projets: HashMap<String, Projet> = HashMap::new();
    if !ids.is_empty() {
        let adresse = format!("{}/projects?ids={}", ext::API, ext::encoder(&serde_json::to_string(&ids).unwrap_or_default()));
        if let Ok(Some(o)) = reseau::telecharger(&agent, &adresse) {
            projets = ext::lire_projets(&o).into_iter().map(|p| (p.id.clone(), p)).collect();
        }
    }
    // La version la plus récente pour ce logiciel et cette version de Minecraft.
    let reconnus: Vec<&String> = manquants.iter().filter(|h| versions.contains_key(*h)).collect();
    let mut dernieres: HashMap<String, VersionMr> = HashMap::new();
    if !reconnus.is_empty() {
        let mut corps = json!({ "hashes": reconnus, "algorithm": "sha1", "loaders": loaders_de(c) });
        if !version.is_empty() {
            corps["game_versions"] = json!([version]);
        }
        if let Ok(o) = reseau::envoyer_json(&agent, &format!("{}/version_files/update", ext::API), &corps) {
            dernieres = ext::lire_par_empreinte(&o);
        }
    }
    let mut cache = etat.catalogue.cache.lock().unwrap();
    for h in &manquants {
        let v = versions.get(h).cloned();
        let projet = v.as_ref().and_then(|v| projets.get(&v.projet).cloned());
        cache.noter(h, cle, version, v, projet, dernieres.get(h).cloned());
    }
    Ok(())
}

fn vue(app: &AppHandle, c: &Contexte) -> VueExtensions {
    let etat = app.state::<Etat>();
    let (logiciel, cle, version) = (c.serveur.logiciel.as_str(), c.cle.as_str(), c.serveur.version.as_str());
    let loaders = loaders_de(c);
    let fichiers = ext::lister(&c.dossier, c.genre);
    let empreintes: Vec<Option<String>> = fichiers.iter().map(|f| if f.dossier { None } else { ext::sha1_fichier(&c.dossier.join(&f.nom)) }).collect();
    let connues: Vec<String> = empreintes.iter().flatten().cloned().collect();
    let hors_ligne = identifier(app, c, &connues).is_err();
    // Avant les verrous du catalogue : demarrer_avec tient `vivants` puis prend `pannes` (demarre).
    let redemarrer = en_ligne(&etat, &c.serveur.id).is_some() && etat.catalogue.a_redemarrer.lock().unwrap().contains(&c.serveur.id);
    // Ce qu'on lit dans les fichiers, aussi avant les verrous (ça peut être long).
    let java = crate::java_du_serveur(&c.serveur);
    let locales: Vec<ext::InfosLocales> = fichiers.iter().map(|f| ext::infos_locales(&c.dossier.join(&f.nom))).collect();
    let lies = lies_au_monde(c, &fichiers);
    let manquants = manquants(app, &c.serveur.id);
    // Combien il y en a dans chaque genre (pour « Plugins 6 · Datapacks 2 »).
    let nombres: HashMap<&'static str, usize> = c
        .genres
        .iter()
        .map(|g| (g.id(), if *g == c.genre { fichiers.len() } else { ext::lister(&ext::dossier(Path::new(&c.serveur.dossier), *g), *g).len() }))
        .collect();
    let cache = etat.catalogue.cache.lock().unwrap();
    let liens = etat.catalogue.liens.lock().unwrap();
    let pannes = etat.catalogue.pannes.lock().unwrap();
    let pannes = pannes.get(&c.serveur.id).map(|p| p.liste.as_slice()).unwrap_or_default();
    let trop_recentes = etat.catalogue.trop_recentes.lock().unwrap();
    let connus: Vec<Option<&ext::Connu>> = empreintes.iter().map(|h| h.as_deref().and_then(|h| cache.lire(h, cle, version))).collect();
    // En double : deux fichiers actifs qui sont le même plugin (même identifiant, ou même projet du catalogue).
    let cles: Vec<(String, Option<String>, bool)> = fichiers
        .iter()
        .zip(&locales)
        .zip(&connus)
        .map(|((f, l), k)| (f.nom.clone(), l.cle().or_else(|| k.and_then(|k| k.version.as_ref()).map(|v| format!("catalogue:{}", v.projet))), f.actif))
        .collect();
    let doubles = ext::doublons(&cles);
    // Les projets actifs, pour les incompatibilités déclarées par leurs auteurs (dans les deux sens).
    let actifs: Vec<(&VersionMr, String)> = fichiers
        .iter()
        .zip(&connus)
        .filter(|(f, _)| f.actif)
        .filter_map(|(_, k)| {
            let k = (*k)?;
            let v = k.version.as_ref()?;
            Some((v, k.projet.as_ref().map(|p| p.nom.clone()).unwrap_or_else(|| v.projet.clone())))
        })
        .collect();
    let mut liste: Vec<Extension> = fichiers
        .iter()
        .zip(&empreintes)
        .zip(locales)
        .zip(&connus)
        .map(|(((f, h), local), connu)| {
            let connu = *connu;
            let v = connu.and_then(|k| k.version.as_ref());
            let projet = connu.and_then(|k| k.projet.as_ref());
            // Pas de bêta pour remplacer une version stable, ni de version qui demande un Java trop récent.
            let maj = connu
                .and_then(|k| k.derniere.as_ref())
                .filter(|d| Some(&d.fichier.sha1) != h.as_ref() && v.is_none_or(|v| v.id != d.id))
                .filter(|d| d.stable || v.is_some_and(|v| !v.stable))
                .filter(|d| !trop_recentes.contains_key(&cle_java(&d.id, java)))
                .map(|d| d.numero.clone());
            let alerte = v.and_then(|v| {
                if !version.is_empty() && !v.jeux.is_empty() && !v.jeux.iter().any(|j| j == version) {
                    Some(format!("Pas prévu pour {version}."))
                } else if !v.loaders.is_empty() && !v.loaders.iter().any(|l| loaders.contains(&l.as_str())) {
                    Some(format!("Pas prévu pour {logiciel}."))
                } else {
                    None
                }
            });
            let conflit = if let Some(autre) = doubles.get(&f.nom) {
                Some(format!("En double avec {autre}."))
            } else if let (Some(v), true) = (v, f.actif) {
                actifs
                    .iter()
                    .find(|(a, _)| a.projet != v.projet && (v.incompatibles.contains(&a.projet) || a.incompatibles.contains(&v.projet)))
                    .map(|(_, nom)| format!("Ne va pas avec {nom}."))
            } else {
                None
            };
            let nom_fichier = ext::nom_actif(&f.nom);
            let sans_ext = nom_fichier.rsplit_once('.').map(|(a, _)| a).filter(|_| !f.dossier).unwrap_or(nom_fichier);
            // En panne si la console a cité son fichier ou son nom (celui de plugin.yml, ou celui du catalogue).
            let noms: Vec<String> = [Some(nom_fichier.to_string()), local.nom.clone(), projet.map(|p| p.nom.clone())].into_iter().flatten().map(|n| n.to_lowercase()).collect();
            let trouvee = f.actif.then(|| pannes.iter().find(|(cle, ..)| noms.contains(cle))).flatten();
            let panne = trouvee.map(|(_, raison, en_jeu)| {
                let raison = raison.as_deref().unwrap_or("Regarde la console.");
                if *en_jeu { format!("Plante en jeu. {raison}") } else { format!("Ne démarre pas. {raison}") }
            });
            let panne_en_jeu = trouvee.is_some_and(|(.., en_jeu)| *en_jeu);
            Extension {
                fichier: f.nom.clone(),
                nom: projet.map(|p| p.nom.clone()).or(local.nom).unwrap_or_else(|| sans_ext.to_string()),
                version: v.map(|v| v.numero.clone()).or(local.version),
                description: projet.map(|p| p.description.clone()).filter(|d| !d.is_empty()).or(local.description),
                couleur: projet.and_then(|p| p.couleur),
                actif: f.actif,
                dossier: f.dossier,
                maj,
                pour: liens.pour(&c.serveur.id, &f.nom),
                alerte,
                projet: v.map(|v| v.projet.clone()),
                icone: projet.and_then(|p| p.icone.clone()),
                panne,
                panne_en_jeu,
                conflit,
                monde: lies.contains(&f.nom),
            }
        })
        .collect();
    liste.sort_by_key(|e| e.nom.to_lowercase());
    VueExtensions {
        genre: c.genre.id(),
        genres: c.genres.iter().map(|g| g.id()).collect(),
        nombres,
        monde: (c.genre == Genre::Datapacks).then(|| crate::mondes::niveau(Path::new(&c.serveur.dossier))),
        liste,
        redemarrer,
        hors_ligne,
        manquants,
    }
}

/// Les datapacks qui ont façonné le terrain du monde : actifs, notés dans level.dat, et qui touchent à la génération.
fn lies_au_monde(c: &Contexte, fichiers: &[Fichier]) -> HashSet<String> {
    if c.genre != Genre::Datapacks {
        return HashSet::new();
    }
    let Some(monde) = c.dossier.parent() else { return HashSet::new() };
    let packs = ext::packs_du_monde(monde);
    fichiers
        .iter()
        .filter(|f| f.actif && packs.iter().any(|p| p.strip_prefix("file/") == Some(f.nom.as_str())) && ext::change_le_monde(&c.dossier.join(&f.nom)))
        .map(|f| f.nom.clone())
        .collect()
}

/// Le monde ne se charge plus (vu au dernier démarrage) : les datapacks qui lui manquent.
fn manquants(app: &AppHandle, id: &str) -> Vec<Manquant> {
    let etat = app.state::<Etat>();
    let pannes = etat.catalogue.pannes.lock().unwrap();
    let Some(p) = pannes.get(id).filter(|p| p.bloque) else { return vec![] };
    p.manquants.iter().map(|f| Manquant { fichier: f.clone(), nom: ext::nom_probable(f) }).collect()
}

/// Avant de couper ou retirer un datapack : s'il a façonné le monde, il faut une confirmation.
/// Note aussi son projet du catalogue, pour pouvoir le remettre plus tard.
fn avant_de_retirer(app: &AppHandle, c: &Contexte, f: &Fichier, forcer: bool) -> Result<(), String> {
    if c.genre != Genre::Datapacks || f.dossier {
        return Ok(());
    }
    if !forcer && lies_au_monde(c, std::slice::from_ref(f)).contains(&f.nom) {
        return Err("Ce datapack a créé le terrain de ce monde : sans lui, le monde ne se chargera plus.".into());
    }
    let Some(h) = ext::sha1_fichier(&c.dossier.join(&f.nom)) else { return Ok(()) };
    let etat = app.state::<Etat>();
    let projet = etat.catalogue.cache.lock().unwrap().lire(&h, &c.cle, &c.serveur.version).and_then(|k| k.version.as_ref().map(|v| v.projet.clone()));
    if let Some(projet) = projet {
        let mut retires = etat.catalogue.retires.lock().unwrap();
        retires.insert(format!("{}|{}", c.serveur.id, ext::nom_actif(&f.nom)), projet);
        if let Ok(t) = serde_json::to_vec(&*retires) {
            let _ = std::fs::write(&etat.catalogue.fichier_retires, t);
        }
    }
    Ok(())
}

/// Un changement pendant que le serveur tourne : il faudra le redémarrer.
fn change(app: &AppHandle, id: &str) {
    let etat = app.state::<Etat>();
    if en_ligne(&etat, id).is_some() {
        etat.catalogue.a_redemarrer.lock().unwrap().insert(id.to_string());
    }
}

/// Un fichier qui existe vraiment dans le dossier (le nom vient de l'interface).
fn trouver(c: &Contexte, fichier: &str) -> Result<Fichier, String> {
    ext::lister(&c.dossier, c.genre).into_iter().find(|f| f.nom == fichier).ok_or_else(|| "Ce fichier n'existe plus.".to_string())
}

#[tauri::command]
pub async fn extensions_serveur(app: AppHandle, id: String, genre: Option<String>) -> Result<VueExtensions, String> {
    a_part(move || {
        let c = contexte(&app, &id, genre.as_deref())?;
        Ok(vue(&app, &c))
    })
    .await
}

/// Une page du catalogue.
#[derive(Serialize)]
pub struct Page {
    projets: Vec<Projet>,
    total: u64,
    /// Il reste des résultats après cette page.
    suite: bool,
}

/// Ajouter : ce que le catalogue propose pour ce logiciel et cette version, page par page (`page` 0, 1, 2…).
#[tauri::command]
pub async fn chercher_extensions(app: AppHandle, id: String, texte: String, categorie: Option<String>, tri: Option<String>, page: Option<u64>, genre: Option<String>) -> Result<Page, String> {
    a_part(move || {
        let c = contexte(&app, &id, genre.as_deref())?;
        let texte: String = texte.chars().take(100).collect();
        let tri = Tri::de(tri.as_deref().unwrap_or(""));
        let page = page.unwrap_or(0).min(500);
        let chercher = |decalage: u64, nombre: u64| -> Result<ext::Recherche, String> {
            let adresse = ext::adresse_recherche(c.genre, &c.serveur.logiciel, &c.serveur.version, &texte, categorie.as_deref(), tri, decalage, nombre);
            let reponse = reseau::telecharger(&agent(&app), &adresse).map_err(|_| "Pas de connexion. Réessaie.".to_string())?.unwrap_or_default();
            ext::lire_recherche(&reponse)
        };
        if tri != Tri::MoinsTelecharges {
            let r = chercher(page * ext::PAGE, ext::PAGE)?;
            let suite = r.decalage + (r.projets.len() as u64) < r.total && r.projets.len() as u64 == ext::PAGE;
            return Ok(Page { projets: r.projets, total: r.total, suite });
        }
        // Les moins téléchargés : la fin de la liste, lue à l'envers.
        let (projets, total, suite) = ext::fin_de_liste(page, chercher)?;
        Ok(Page { projets, total, suite })
    })
    .await
}

/// La clé de « trop récente » : une version n'est trop récente que pour un Java donné.
fn cle_java(version: &str, java: Option<u32>) -> String {
    format!("{version}|{}", java.unwrap_or(0))
}

/// Télécharge un fichier du catalogue et vérifie son empreinte.
fn telecharger_version(app: &AppHandle, v: &VersionMr) -> Result<Vec<u8>, String> {
    let octets = reseau::telecharger_gros(&agent(app), &v.fichier.url, ext::TAILLE_MAX).map_err(|_| "Le téléchargement a échoué. Réessaie.".to_string())?;
    if !ext::verifier(&octets, &v.fichier) {
        return Err("Fichier abîmé pendant le téléchargement. Réessaie.".into());
    }
    Ok(octets)
}

/// Écrit un fichier téléchargé dans le dossier. Renvoie son nom.
fn poser(c: &Contexte, v: &VersionMr, octets: &[u8], coupe: bool) -> Result<String, String> {
    std::fs::create_dir_all(&c.dossier).map_err(|_| "Le dossier n'a pas pu être créé.".to_string())?;
    let nom = if coupe { format!("{}.disabled", v.fichier.nom) } else { v.fichier.nom.clone() };
    std::fs::write(c.dossier.join(&nom), octets).map_err(|_| "Windows refuse d'écrire le fichier.".to_string())?;
    Ok(nom)
}

/// Les versions d'un projet qui marchent sur ce serveur, de la plus récente à la plus ancienne.
fn versions_pour(app: &AppHandle, c: &Contexte, projet: &str) -> Result<Vec<VersionMr>, String> {
    let adresse = ext::adresse_versions(projet, &loaders_de(c), &c.serveur.version);
    let reponse = reseau::telecharger(&agent(app), &adresse).map_err(|_| "Pas de connexion. Réessaie.".to_string())?.unwrap_or_default();
    Ok(ext::lire_versions(&reponse))
}

/// Ce que `choisir` a trouvé.
enum Choix {
    /// La meilleure version, déjà téléchargée et vérifiée.
    Version(Box<VersionMr>, Vec<u8>),
    /// Rien de mieux que la version actuelle (ou aucune version pour ce serveur).
    Rien,
}

/// Choisit la version à installer : la plus récente (stable d'abord) dont le Java tourne sur ce serveur.
/// Une version compilée pour un Java plus récent que celui du serveur ne démarrerait pas : on prend la précédente.
/// `actuelle` : l'identifiant de la version installée (pour une mise à jour : seulement les plus récentes qu'elle).
fn choisir(app: &AppHandle, c: &Contexte, projet: &str, actuelle: Option<&str>) -> Result<Choix, String> {
    let java = crate::java_du_serveur(&c.serveur);
    let mut versions = versions_pour(app, c, projet)?;
    if let Some(a) = actuelle {
        versions = ext::plus_recentes(versions, a);
    }
    let versions = ext::ordre_essai(versions);
    // Le verrou n'est pas gardé pendant les téléchargements : copie, puis on rend ce qu'on a appris.
    let etat = app.state::<Etat>();
    let mut refusees = etat.catalogue.trop_recentes.lock().unwrap().clone();
    let avant = refusees.len();
    let essai = ext::choisir_version(versions, actuelle, java, &mut refusees, 8, |v| {
        let octets = telecharger_version(app, v)?;
        let il_faut = if c.genre == Genre::Datapacks { None } else { ext::java_du_jar(&octets) };
        Ok((octets, il_faut))
    });
    if refusees.len() != avant {
        let mut garde = etat.catalogue.trop_recentes.lock().unwrap();
        garde.extend(refusees);
        if let Ok(t) = serde_json::to_vec(&*garde) {
            let _ = std::fs::write(&etat.catalogue.fichier_trop_recentes, t);
        }
    }
    match essai? {
        ext::Essai::Version(v, octets) => Ok(Choix::Version(Box::new(v), octets)),
        ext::Essai::DejaLa | ext::Essai::Aucune => Ok(Choix::Rien),
        ext::Essai::TropRecente(il_faut) if actuelle.is_none() => {
            Err(format!("Il lui faut Java {il_faut} (ton serveur : Java {}).", java.unwrap_or(0)))
        }
        ext::Essai::TropRecente(_) => Ok(Choix::Rien),
    }
}

fn nom_projet(app: &AppHandle, projet: &str) -> String {
    reseau::telecharger(&agent(app), &format!("{}/project/{projet}", ext::API))
        .ok()
        .flatten()
        .and_then(|o| serde_json::from_slice::<serde_json::Value>(&o).ok())
        .and_then(|v| v.get("title").and_then(|t| t.as_str()).map(String::from))
        .unwrap_or_else(|| projet.to_string())
}

#[derive(Serialize)]
pub struct Installation {
    vue: VueExtensions,
    /// Les dépendances installées avec (« Fabric API »).
    avec: Vec<String>,
    /// Les dépendances introuvables pour ce serveur.
    manque: Vec<String>,
}

/// Le résultat d'une mise à jour : la liste, et combien de fichiers ont vraiment changé.
#[derive(Serialize)]
pub struct MiseAJour {
    vue: VueExtensions,
    faits: u32,
}

/// Installe un projet du catalogue, avec ce qu'il lui faut (dépendances obligatoires, 3 niveaux au plus).
#[tauri::command]
pub async fn installer_extension(app: AppHandle, id: String, projet: String, genre: Option<String>) -> Result<Installation, String> {
    a_part(move || {
        if !ext::id_valide(&projet) {
            return Err("Projet inconnu.".into());
        }
        let c = contexte(&app, &id, genre.as_deref())?;
        installer(&app, &c, &projet)
    })
    .await
}

/// Installe un projet et ses dépendances obligatoires (voir `installer_extension`).
fn installer(app: &AppHandle, c: &Contexte, projet: &str) -> Result<Installation, String> {
    let (app, id, projet) = (app.clone(), c.serveur.id.clone(), projet.to_string());
    let avant = vue(&app, c);
    let mut presents: HashSet<String> = avant.liste.iter().filter_map(|e| e.projet.clone()).collect();
    if presents.contains(&projet) {
        return Err("Déjà installé.".into());
    }
    let nom = nom_projet(&app, &projet);
    // Ce qui est déjà là : l'identifiant de chaque plugin (plugin.yml…), même ceux que le catalogue ne connaît pas,
    // et les versions actives connues du catalogue (pour leurs incompatibilités).
    let installes = ext::lister(&c.dossier, c.genre);
    let ids_locaux: HashMap<String, String> = installes
        .iter()
        .filter_map(|f| Some((ext::infos_locales(&c.dossier.join(&f.nom)).cle()?, f.nom.clone())))
        .collect();
    let actives: Vec<(VersionMr, String)> = {
        let etat = app.state::<Etat>();
        let cache = etat.catalogue.cache.lock().unwrap();
        installes
            .iter()
            .filter(|f| f.actif && !f.dossier)
            .filter_map(|f| {
                let h = ext::sha1_fichier(&c.dossier.join(&f.nom))?;
                let k = cache.lire(&h, &c.cle, &c.serveur.version)?;
                let v = k.version.clone()?;
                let n = k.projet.as_ref().map(|p| p.nom.clone()).unwrap_or_else(|| v.projet.clone());
                Some((v, n))
            })
            .collect()
    };
    let conflit = |v: &VersionMr| actives.iter().find(|(a, _)| a.projet != v.projet && (v.incompatibles.contains(&a.projet) || a.incompatibles.contains(&v.projet))).map(|(_, n)| n.clone());
    let mut a_faire = vec![(projet.clone(), 0u8)];
    let mut avec = Vec::new();
    let mut manque = Vec::new();
    while let Some((p, niveau)) = a_faire.pop() {
        if !presents.insert(p.clone()) {
            continue;
        }
        let choix = match choisir(&app, c, &p, None) {
            Ok(choix) => choix,
            Err(e) if p == projet => return Err(e),
            Err(_) => {
                manque.push(nom_projet(&app, &p));
                continue;
            }
        };
        let Choix::Version(v, octets) = choix else {
            if p == projet {
                return Err(format!("Pas disponible pour {} {}.", c.serveur.logiciel, c.serveur.version));
            }
            manque.push(nom_projet(&app, &p));
            continue;
        };
        // Déjà posé à la main (même identifiant de plugin, ou même nom de fichier) : on ne le double pas.
        let deja = ext::id_dans(&octets)
            .and_then(|i| ids_locaux.get(&i).cloned())
            .or_else(|| installes.iter().find(|f| ext::nom_actif(&f.nom).eq_ignore_ascii_case(&v.fichier.nom)).map(|f| f.nom.clone()));
        if let Some(fichier) = deja {
            if p == projet {
                return Err(format!("Déjà là : {fichier}."));
            }
            continue;
        }
        // Incompatible (d'après son auteur ou celui d'un plugin déjà là).
        if let Some(autre) = conflit(&v) {
            if p == projet {
                return Err(format!("Ne va pas avec {autre}."));
            }
            manque.push(format!("{} (ne va pas avec {autre})", nom_projet(&app, &p)));
            continue;
        }
        let fichier = poser(c, &v, &octets, false)?;
        if p != projet {
            avec.push(nom_projet(&app, &p));
            let etat = app.state::<Etat>();
            let mut liens = etat.catalogue.liens.lock().unwrap();
            liens.noter(&id, &fichier, &nom);
            liens.enregistrer(&etat.catalogue.fichier_liens);
        }
        if niveau < 3 {
            a_faire.extend(v.dependances.iter().map(|d| (d.clone(), niveau + 1)));
        }
    }
    change(&app, &id);
    Ok(Installation { vue: vue(&app, c), avec, manque })
}

/// Active ou coupe une extension.
#[tauri::command]
pub async fn activer_extension(app: AppHandle, id: String, fichier: String, actif: bool, genre: Option<String>, forcer: Option<bool>) -> Result<VueExtensions, String> {
    a_part(move || {
        let c = contexte(&app, &id, genre.as_deref())?;
        let f = trouver(&c, &fichier)?;
        if f.dossier {
            return Err("Ce datapack ne peut pas être coupé.".into());
        }
        if !actif {
            avant_de_retirer(&app, &c, &f, forcer.unwrap_or(false))?;
        }
        ext::basculer(&c.dossier, &f.nom, actif)?;
        change(&app, &id);
        Ok(vue(&app, &c))
    })
    .await
}

/// Désinstalle une extension : le fichier va à la Corbeille.
#[tauri::command]
pub async fn desinstaller_extension(app: AppHandle, id: String, fichier: String, genre: Option<String>, forcer: Option<bool>) -> Result<VueExtensions, String> {
    a_part(move || {
        let c = contexte(&app, &id, genre.as_deref())?;
        let f = trouver(&c, &fichier)?;
        avant_de_retirer(&app, &c, &f, forcer.unwrap_or(false))?;
        crate::corbeille::jeter(&c.dossier.join(&f.nom)).map_err(|_| "Windows refuse : arrête le serveur d'abord.".to_string())?;
        {
            let etat = app.state::<Etat>();
            let mut liens = etat.catalogue.liens.lock().unwrap();
            liens.oublier_fichier(&id, &f.nom);
            liens.enregistrer(&etat.catalogue.fichier_liens);
        }
        change(&app, &id);
        Ok(vue(&app, &c))
    })
    .await
}

// ---------------------------------------------------------------------------------------------
// Paramètres › Joueurs Bedrock (Geyser + Floodgate, voir bedrock.rs)
// ---------------------------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VueBedrock {
    /// Possible sur ce serveur (logiciel et version).
    possible: bool,
    /// Sinon, pourquoi (« Pas possible avec Forge. »).
    raison: Option<String>,
    /// Geyser et Floodgate sont là et actifs.
    actif: bool,
    /// Le port Bedrock de ce serveur.
    port: u16,
    /// Le serveur tourne : le changement compte au prochain démarrage.
    redemarrer: bool,
    /// Ce qui n'a pas pu être installé (« ViaVersion »), sans empêcher le reste.
    #[serde(skip_serializing_if = "Option::is_none")]
    note: Option<String>,
}

fn vue_bedrock(app: &AppHandle, s: &Serveur, note: Option<String>) -> VueBedrock {
    let port = s.bedrock_port.unwrap_or(crate::bedrock::PORT_BEDROCK);
    match crate::bedrock::possible(&s.logiciel, &s.version) {
        Err(raison) => VueBedrock { possible: false, raison: Some(raison.into()), actif: false, port, redemarrer: false, note },
        Ok(_) => {
            let etat = app.state::<Etat>();
            let redemarrer = en_ligne(&etat, &s.id).is_some() && etat.catalogue.a_redemarrer.lock().unwrap().contains(&s.id);
            let actif = crate::bedrock::actif(Path::new(&s.dossier), &s.logiciel, &s.version);
            VueBedrock { possible: true, raison: None, actif, port, redemarrer, note }
        }
    }
}

#[tauri::command]
pub fn bedrock_serveur(app: AppHandle, id: String) -> Result<VueBedrock, String> {
    let s = serveur_de(&app.state::<Etat>(), &id)?;
    Ok(vue_bedrock(&app, &s, None))
}

/// Télécharge la dernière version de Geyser ou Floodgate chez GeyserMC (vérifiée en SHA-256), dans le dossier des plugins.
fn telecharger_geysermc(app: &AppHandle, c: &Contexte, projet: &str) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let agent = agent(app);
    let plateforme = crate::bedrock::plateforme(&c.serveur.logiciel);
    let json = reseau::telecharger(&agent, &crate::bedrock::adresse_build(projet)).map_err(|_| "Pas de connexion. Réessaie.".to_string())?.ok_or("GeyserMC ne répond pas.")?;
    let (nom, sha) = crate::bedrock::lire_build(&json, plateforme).ok_or("GeyserMC a répondu bizarrement. Réessaie plus tard.")?;
    let octets = reseau::telecharger_gros(&agent, &crate::bedrock::adresse_fichier(projet, plateforme), 64 * 1024 * 1024).map_err(|_| "Téléchargement coupé. Réessaie.".to_string())?;
    let obtenu: String = Sha256::digest(&octets).iter().map(|b| format!("{b:02x}")).collect();
    if obtenu != sha {
        return Err("Le fichier téléchargé est abîmé. Réessaie.".into());
    }
    std::fs::create_dir_all(&c.dossier).map_err(|_| "Le dossier n'a pas pu être créé.".to_string())?;
    std::fs::write(c.dossier.join(&nom), &octets).map_err(|_| "Windows refuse d'écrire le fichier.".to_string())?;
    Ok(nom)
}

/// Autorise (ou non) les joueurs Bedrock : installe ou rallume Geyser et Floodgate (et ViaVersion pour les plugins),
/// choisit un port, coupe la signature du chat (sinon les joueurs Bedrock ne peuvent pas parler).
/// Couper : Geyser et Floodgate sont seulement coupés (« .disabled »), pour revenir vite.
#[tauri::command]
pub async fn regler_bedrock(app: AppHandle, id: String, actif: bool) -> Result<VueBedrock, String> {
    a_part(move || {
        let etat = app.state::<Etat>();
        let s = serveur_de(&etat, &id)?;
        let genre = crate::bedrock::possible(&s.logiciel, &s.version)?;
        let _travail = crate::commencer_travail(&etat, &id)?;
        let c = contexte(&app, &id, Some(genre.id()))?;
        let pieces = crate::bedrock::Pieces::lire(&ext::lister(&c.dossier, genre));
        let mut note = None;
        if !actif {
            for f in [&pieces.geyser, &pieces.floodgate].into_iter().flatten().filter(|f| f.actif) {
                ext::basculer(&c.dossier, &f.nom, false)?;
            }
        } else {
            // Geyser puis Floodgate : rallumés s'ils sont coupés, sinon installés.
            for (piece, projet, catalogue) in [(&pieces.geyser, "geyser", crate::bedrock::GEYSER), (&pieces.floodgate, "floodgate", crate::bedrock::FLOODGATE)] {
                match piece {
                    Some(f) if !f.actif => {
                        ext::basculer(&c.dossier, &f.nom, true)?;
                    }
                    Some(_) => {}
                    None if genre == Genre::Plugins => {
                        telecharger_geysermc(&app, &c, projet)?;
                    }
                    None => {
                        installer(&app, &c, catalogue).map_err(|e| {
                            if e.starts_with("Pas disponible") { format!("Geyser marche seulement avec la dernière version de Minecraft (pas {}).", s.version) } else { e }
                        })?;
                    }
                }
            }
            // ViaVersion : laisse entrer Geyser, qui se présente avec la dernière version de Minecraft.
            if genre == Genre::Plugins {
                match &pieces.via {
                    Some(f) if !f.actif => {
                        ext::basculer(&c.dossier, &f.nom, true)?;
                    }
                    Some(_) => {}
                    None => {
                        let mut via = contexte(&app, &id, Some(genre.id()))?;
                        via.loaders = Some(vec!["paper"]);
                        if installer(&app, &via, crate::bedrock::VIAVERSION).is_err() {
                            note = Some("ViaVersion n'a pas pu être installé : les joueurs Bedrock ne pourront peut-être pas entrer.".to_string());
                        }
                    }
                }
            }
            // Un port à lui (19132, ou le suivant si un autre serveur le prend).
            if s.bedrock_port.is_none() {
                let mut liste = etat.liste.lock().unwrap();
                let pris: Vec<u16> = liste.iter().filter(|x| x.id != id).filter_map(|x| x.bedrock_port).collect();
                if let Some(x) = liste.iter_mut().find(|x| x.id == id) {
                    x.bedrock_port = Some(crate::bedrock::port_libre(&pris));
                }
                crate::serveurs::enregistrer(&etat.fichier, &liste)?;
            }
            // Sans ça, les joueurs Bedrock ne peuvent ni entrer ni parler (signature du chat).
            let _ = crate::serveurs::modifier_proprietes(Path::new(&s.dossier), &[("enforce-secure-profile", "false".to_string())]);
        }
        change(&app, &id);
        let s = serveur_de(&etat, &id)?;
        // La box : on ouvre (ou referme) aussi le port Bedrock.
        if s.ouvert_amis {
            let _ = app.emit("acces", etat.acces.reveiller(&id, true));
        }
        Ok(vue_bedrock(&app, &s, note))
    })
    .await
}

/// Le monde de ce serveur ne se charge plus : les datapacks qui lui manquent (pour l'Accueil).
#[tauri::command]
pub fn datapacks_manquants(app: AppHandle, id: String) -> Vec<Manquant> {
    manquants(&app, &id)
}

/// Remet un datapack que le monde attend : on le rallume s'il est coupé, sinon on le réinstalle
/// depuis le catalogue (le projet noté quand il a été retiré, sinon une recherche par son nom).
#[tauri::command]
pub async fn remettre_datapack(app: AppHandle, id: String, fichier: String) -> Result<String, String> {
    if !ext::nom_fichier_valide(&fichier) {
        return Err("Fichier inconnu.".into());
    }
    enum Quoi {
        Rallume,
        Installer(String),
    }
    let (app2, id2, fichier2) = (app.clone(), id.clone(), fichier.clone());
    let quoi = a_part(move || {
        let c = contexte(&app2, &id2, Some("datapacks"))?;
        let coupe = format!("{fichier2}.disabled");
        if c.dossier.join(&coupe).is_file() {
            ext::basculer(&c.dossier, &coupe, true)?;
            return Ok(Quoi::Rallume);
        }
        if c.dossier.join(&fichier2).exists() {
            return Ok(Quoi::Rallume);
        }
        let etat = app2.state::<Etat>();
        let note = etat.catalogue.retires.lock().unwrap().get(&format!("{id2}|{fichier2}")).cloned();
        if let Some(projet) = note {
            return Ok(Quoi::Installer(projet));
        }
        // Pas noté (retiré avant cette version de BHOST, ou à la main) : on le cherche par son nom.
        let nom = ext::nom_probable(&fichier2);
        let adresse = ext::adresse_recherche(Genre::Datapacks, &c.serveur.logiciel, &c.serveur.version, &nom, None, Tri::Pertinence, 0, 5);
        let reponse = reseau::telecharger(&agent(&app2), &adresse).map_err(|_| "Pas de connexion. Réessaie.".to_string())?.unwrap_or_default();
        let projets = ext::lire_recherche(&reponse)?.projets;
        let simple = |t: &str| t.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase();
        let voulu = simple(&nom);
        projets
            .iter()
            .find(|p| simple(&p.nom) == voulu)
            .or_else(|| projets.iter().find(|p| !voulu.is_empty() && simple(&p.nom).starts_with(&voulu)))
            .map(|p| Quoi::Installer(p.id.clone()))
            .ok_or_else(|| format!("{nom} est introuvable dans le catalogue. Remets le fichier {fichier2} dans le dossier datapacks."))
    })
    .await?;
    if let Quoi::Installer(projet) = quoi {
        match installer_extension(app.clone(), id.clone(), projet, Some("datapacks".into())).await {
            Ok(_) => {}
            Err(e) if e.starts_with("Déjà") => {}
            Err(e) => return Err(e),
        }
    }
    // Il n'est plus manquant : le message disparaît.
    {
        let etat = app.state::<Etat>();
        let mut pannes = etat.catalogue.pannes.lock().unwrap();
        if let Some(p) = pannes.get_mut(&id) {
            p.manquants.retain(|m| *m != fichier);
            if p.manquants.is_empty() {
                p.bloque = false;
            }
        }
    }
    let _ = app.emit("extensions", &id);
    Ok(ext::nom_probable(&fichier))
}

/// Met à jour des extensions (toutes celles qui ont une mise à jour si `fichiers` est vide).
#[tauri::command]
pub async fn mettre_a_jour_extensions(app: AppHandle, id: String, fichiers: Vec<String>, genre: Option<String>) -> Result<MiseAJour, String> {
    a_part(move || {
        let c = contexte(&app, &id, genre.as_deref())?;
        let (faits, resultat) = mettre_a_jour(&app, &c, &fichiers);
        resultat?;
        Ok(MiseAJour { vue: vue(&app, &c), faits: faits.len() as u32 })
    })
    .await
}

/// Met à jour ces fichiers (tous si la liste est vide), avec la meilleure version pour ce serveur.
/// Renvoie les fichiers mis à jour (leur ancien nom), même si une erreur arrive au milieu.
fn mettre_a_jour(app: &AppHandle, c: &Contexte, fichiers: &[String]) -> (Vec<String>, Result<(), String>) {
    let id = c.serveur.id.as_str();
    let (cle, version) = (c.cle.clone(), c.serveur.version.clone());
    let installes = ext::lister(&c.dossier, c.genre);
    let mut faits = Vec::new();
    // Une erreur au milieu : ce qui est déjà fait compte quand même (note « Redémarre »).
    let resultat = (|| -> Result<(), String> {
        for f in installes.iter().filter(|f| !f.dossier && (fichiers.is_empty() || fichiers.contains(&f.nom))) {
            let Some(h) = ext::sha1_fichier(&c.dossier.join(&f.nom)) else { continue };
            identifier(app, c, std::slice::from_ref(&h))?;
            // Seulement ce que la liste propose (une mise à jour connue), puis la meilleure version pour ce serveur.
            let (actuelle, projet) = {
                let etat = app.state::<Etat>();
                let cache = etat.catalogue.cache.lock().unwrap();
                let Some(k) = cache.lire(&h, &cle, &version) else { continue };
                if k.derniere.as_ref().is_none_or(|d| d.fichier.sha1 == h) {
                    continue;
                }
                let Some(v) = k.version.as_ref() else { continue };
                (v.id.clone(), v.projet.clone())
            };
            let Choix::Version(d, octets) = choisir(app, c, &projet, Some(&actuelle))? else { continue };
            if d.fichier.sha1 == h {
                continue;
            }
            let nouveau = poser(c, &d, &octets, !f.actif)?;
            // Windows ne distingue pas « WorldEdit.jar » de « worldedit.jar » : c'est le même fichier.
            if !nouveau.eq_ignore_ascii_case(&f.nom) {
                if std::fs::remove_file(c.dossier.join(&f.nom)).is_err() {
                    // L'ancien est bloqué (serveur allumé) : on retire le nouveau pour ne pas avoir les deux.
                    let _ = std::fs::remove_file(c.dossier.join(&nouveau));
                    return Err("Windows refuse : arrête le serveur d'abord.".into());
                }
                let etat = app.state::<Etat>();
                let mut liens = etat.catalogue.liens.lock().unwrap();
                liens.renommer(id, &f.nom, &nouveau);
                liens.enregistrer(&etat.catalogue.fichier_liens);
            }
            faits.push(f.nom.clone());
        }
        Ok(())
    })();
    if !faits.is_empty() {
        change(app, id);
    }
    (faits, resultat)
}

/// Une mise à jour disponible (pour l'automatisation « Une mise à jour sort »).
#[derive(Debug, Clone, PartialEq)]
pub struct Dispo {
    pub genre: Genre,
    pub fichier: String,
    pub nom: String,
    /// La version proposée (« 2.21.1 »).
    pub version: String,
}

/// Les plugins et mods actifs qui ont une mise à jour qui marche sur ce serveur (pas les datapacks :
/// certains créent le terrain du monde).
pub fn mises_a_jour_dispo(app: &AppHandle, id: &str) -> Result<Vec<Dispo>, String> {
    let serveur = serveur_de(&app.state::<Etat>(), id)?;
    let mut liste = Vec::new();
    for genre in ext::genres(&serveur.logiciel, &serveur.version).into_iter().filter(|g| *g != Genre::Datapacks) {
        let c = contexte(app, id, Some(genre.id()))?;
        let v = vue(app, &c);
        if v.hors_ligne {
            return Err("Le catalogue ne répond pas.".into());
        }
        liste.extend(v.liste.into_iter().filter(|e| e.actif && !e.dossier).filter_map(|e| Some(Dispo { genre, version: e.maj?, fichier: e.fichier, nom: e.nom })));
    }
    Ok(liste)
}

/// Met à jour ces plugins et mods (serveur arrêté). Renvoie les noms de ceux qui ont changé.
pub fn appliquer_mises_a_jour(app: &AppHandle, id: &str, dispo: &[Dispo]) -> Result<Vec<String>, String> {
    let mut noms = Vec::new();
    for genre in [Genre::Mods, Genre::Plugins] {
        let fichiers: Vec<String> = dispo.iter().filter(|d| d.genre == genre).map(|d| d.fichier.clone()).collect();
        if fichiers.is_empty() {
            continue;
        }
        let c = contexte(app, id, Some(genre.id()))?;
        let (faits, resultat) = mettre_a_jour(app, &c, &fichiers);
        noms.extend(dispo.iter().filter(|d| d.genre == genre && faits.contains(&d.fichier)).map(|d| d.nom.clone()));
        // Une erreur après une réussite : on garde ce qui est fait.
        if let (Err(e), true) = (resultat, noms.is_empty()) {
            return Err(e);
        }
    }
    if !noms.is_empty() {
        let _ = app.emit("extensions", id);
    }
    Ok(noms)
}
