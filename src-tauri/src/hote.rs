//! Ce qui fait tourner le cœur de BHOST.
//! - Appli Windows (« bureau ») : c'est Tauri (la fenêtre envoie les commandes, reçoit les nouvelles).
//! - Agent de BHOST OS : pas de fenêtre. Les mêmes fonctions sont appelées par `agent::appeler`,
//!   et les nouvelles (`emit`) partent vers BHOST à distance (par le relais).
//! Le reste du code écrit seulement `use crate::hote::{AppHandle, Emitter, Manager, State}`.

#[cfg(feature = "bureau")]
pub use tauri::{AppHandle, Emitter, Manager, State, async_runtime};

#[cfg(not(feature = "bureau"))]
pub use faux::*;

/// Une notification (Réglages › Notifications). `meme_devant` : aussi quand BHOST est au premier plan.
/// Sur BHOST OS, pas d'écran : la nouvelle « notification » part vers BHOST, qui l'affiche.
pub fn notifier(app: &AppHandle, titre: &str, texte: &str, meme_devant: bool) {
    if !crate::NOTIFICATIONS.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    #[cfg(feature = "bureau")]
    {
        use tauri_plugin_notification::NotificationExt;
        let devant = app.get_webview_window("main").and_then(|f| f.is_focused().ok()).unwrap_or(false);
        if meme_devant || !devant {
            let _ = app.notification().builder().title(titre).body(texte).show();
        }
    }
    #[cfg(not(feature = "bureau"))]
    {
        let _ = meme_devant;
        let _ = app.emit("notification", serde_json::json!({ "titre": titre, "texte": texte }));
    }
}

#[cfg(not(feature = "bureau"))]
mod faux {
    use serde::Serialize;
    use std::any::{Any, TypeId};
    use std::collections::HashMap;
    use std::ops::Deref;
    use std::sync::{Arc, RwLock};

    /// Reçoit chaque nouvelle (nom, contenu en JSON) : l'agent l'envoie à BHOST.
    pub type Sortie = Box<dyn Fn(&str, serde_json::Value) + Send + Sync>;

    struct Interne {
        etats: RwLock<HashMap<TypeId, &'static (dyn Any + Send + Sync)>>,
        sortie: Sortie,
    }

    /// Le cœur de BHOST, sans Tauri : les états partagés (comme `app.manage`) et la sortie des nouvelles.
    #[derive(Clone)]
    pub struct AppHandle(Arc<Interne>);

    impl AppHandle {
        pub fn nouveau(sortie: Sortie) -> Self {
            AppHandle(Arc::new(Interne { etats: RwLock::new(HashMap::new()), sortie }))
        }
    }

    /// Un état partagé, comme `tauri::State`.
    pub struct State<'a, T: 'static>(&'a T);

    impl<T> Deref for State<'_, T> {
        type Target = T;
        fn deref(&self) -> &T {
            self.0
        }
    }

    impl<T> Clone for State<'_, T> {
        fn clone(&self) -> Self {
            *self
        }
    }
    impl<T> Copy for State<'_, T> {}

    impl<'a, T: 'static> State<'a, T> {
        pub fn inner(&self) -> &'a T {
            self.0
        }
    }

    /// `app.state::<T>()` et `app.manage(t)`, comme dans Tauri.
    pub trait Manager {
        fn state<T: Send + Sync + 'static>(&self) -> State<'_, T>;
        fn try_state<T: Send + Sync + 'static>(&self) -> Option<State<'_, T>>;
        fn manage<T: Send + Sync + 'static>(&self, valeur: T) -> bool;
    }

    impl Manager for AppHandle {
        fn state<T: Send + Sync + 'static>(&self) -> State<'_, T> {
            self.try_state().unwrap_or_else(|| panic!("état non géré : {}", std::any::type_name::<T>()))
        }
        fn try_state<T: Send + Sync + 'static>(&self) -> Option<State<'_, T>> {
            let etats = self.0.etats.read().unwrap();
            let e: &'static (dyn Any + Send + Sync) = *etats.get(&TypeId::of::<T>())?;
            e.downcast_ref::<T>().map(State)
        }
        fn manage<T: Send + Sync + 'static>(&self, valeur: T) -> bool {
            // L'état vit aussi longtemps que l'agent (comme dans Tauri) : on le « fuit » volontairement.
            let mut etats = self.0.etats.write().unwrap();
            if etats.contains_key(&TypeId::of::<T>()) {
                return false;
            }
            etats.insert(TypeId::of::<T>(), Box::leak(Box::new(valeur)));
            true
        }
    }

    /// `app.emit("statut", …)`, comme dans Tauri.
    pub trait Emitter {
        fn emit<S: Serialize + Clone>(&self, evenement: &str, contenu: S) -> Result<(), String>;
    }

    impl Emitter for AppHandle {
        fn emit<S: Serialize + Clone>(&self, evenement: &str, contenu: S) -> Result<(), String> {
            let json = serde_json::to_value(contenu).map_err(|e| e.to_string())?;
            (self.0.sortie)(evenement, json);
            Ok(())
        }
    }

    /// `crate::hote::async_runtime::spawn_blocking`.
    pub mod async_runtime {
        pub fn spawn_blocking<F, R>(f: F) -> tokio::task::JoinHandle<R>
        where
            F: FnOnce() -> R + Send + 'static,
            R: Send + 'static,
        {
            tokio::task::spawn_blocking(f)
        }
    }
}
