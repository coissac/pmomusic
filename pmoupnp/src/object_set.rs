use std::{collections::HashMap, sync::Arc};

use tokio::sync::RwLock;

use crate::{UpnpDeepClone, UpnpObjectSet, UpnpObjectSetError, UpnpTypedObject};

/// Implémentation du clonage profond pour `UpnpObjectSet`.
///
/// Cette implémentation crée une copie complète et indépendante du set,
/// en clonant chaque objet `T` et en créant de nouveaux `Arc` autour de ces clones.
/// Les modifications sur l'un des sets n'affectent pas l'autre.
impl<T: UpnpTypedObject> UpnpDeepClone for UpnpObjectSet<T> {
    fn deep_clone(&self) -> Self {
        let guard = self.objects.blocking_read();

        let cloned_map: HashMap<String, Arc<T>> = guard
            .iter()
            .map(|(key, arc)| (key.clone(), Arc::new((**arc).clone())))
            .collect();

        Self {
            objects: RwLock::new(cloned_map),
        }
    }
}

/// Implémentation du clonage superficiel pour `UpnpObjectSet`.
///
/// Cette implémentation crée une copie du set qui **partage** les objets `T`
/// via les `Arc`. C'est beaucoup plus rapide et économe en mémoire qu'un clonage
/// profond, car seuls les pointeurs `Arc` sont clonés (incrémentation du compteur
/// de références).
///
/// # Note
///
/// Les deux sets partagent les mêmes instances d'objets `T`. Si `T` contient
/// de la mutabilité interne (via `Mutex`, `RwLock`, etc.), les modifications
/// seront visibles depuis les deux sets.
impl<T: UpnpTypedObject> Clone for UpnpObjectSet<T> {
    fn clone(&self) -> Self {
        let guard = self.objects.blocking_read();

        Self {
            objects: RwLock::new(guard.clone()),
        }
    }
}

impl<T: UpnpTypedObject> UpnpObjectSet<T> {

    pub fn new() -> Self {
        Self {
            objects: RwLock::new(HashMap::new()),
        }
    }


    /// Insère un objet dans le set.
    ///
    /// # Arguments
    ///
    /// * `object` - L'objet à insérer, encapsulé dans un `Arc`
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Si l'insertion a réussi
    /// * `Err(UpnpObjectSetError::AlreadyExists)` - Si un objet avec le même nom existe déjà
    ///
    /// # Examples
    ///
    /// ```
    /// let mut set = UpnpObjectSet::new();
    /// let obj = Arc::new(MyObject::new("test"));
    /// set.insert(obj).await?;
    /// ```
    pub async fn insert(&mut self, object: Arc<T>) -> Result<(), UpnpObjectSetError> {
        let mut guard = self.objects.write().await;
        let key = object.get_name().to_string();

        if guard.contains_key(&key) {
            return Err(UpnpObjectSetError::AlreadyExists(key));
        }

        guard.insert(key, object);
        Ok(())
    }

    /// Insère un objet dans le set, ou remplace l'objet existant s'il y en a un avec le même nom.
    ///
    /// Cette méthode ne retourne jamais d'erreur et écrase silencieusement tout objet existant.
    ///
    /// # Arguments
    ///
    /// * `object` - L'objet à insérer ou remplacer, encapsulé dans un `Arc`
    ///
    /// # Examples
    ///
    /// ```
    /// let mut set = UpnpObjectSet::new();
    /// let obj1 = Arc::new(MyObject::new("test"));
    /// let obj2 = Arc::new(MyObject::new("test")); // Même nom
    /// 
    /// set.insert_or_replace(obj1).await;
    /// set.insert_or_replace(obj2).await; // Remplace obj1
    /// ```
    pub async fn insert_or_replace(&mut self, object: Arc<T>) {
        let mut guard = self.objects.write().await;
        let key: String = object.get_name().to_string();

        guard.insert(key, object);
    }

    /// Vérifie si le set contient un objet donné.
    ///
    /// La vérification se base sur le nom de l'objet retourné par `get_name()`.
    ///
    /// # Arguments
    ///
    /// * `object` - L'objet à rechercher
    ///
    /// # Returns
    ///
    /// `true` si un objet avec le même nom existe dans le set, `false` sinon.
    ///
    /// # Examples
    ///
    /// ```
    /// let set = UpnpObjectSet::new();
    /// let obj = Arc::new(MyObject::new("test"));
    /// 
    /// if set.contains(obj.clone()).await {
    ///     println!("L'objet existe déjà");
    /// }
    /// ```
    pub async fn contains(&self, object: Arc<T>) -> bool {
        let guard = self.objects.read().await;
        let key: String = object.get_name().to_string();

        guard.contains_key(&key)
    }

    /// Récupère un objet par son nom (version asynchrone).
    ///
    /// # Arguments
    ///
    /// * `name` - Le nom de l'objet à rechercher
    ///
    /// # Returns
    ///
    /// * `Some(Arc<T>)` - Si un objet avec ce nom existe
    /// * `None` - Si aucun objet n'est trouvé
    ///
    /// # Examples
    ///
    /// ```
    /// let set = UpnpObjectSet::new();
    /// 
    /// if let Some(obj) = set.get_by_name("test").await {
    ///     println!("Objet trouvé: {}", obj.get_name());
    /// }
    /// ```
    pub async fn get_by_name(&self, name: &str) -> Option<Arc<T>> {
        let guard = self.objects.read().await;
        guard.get(name).cloned()
    }

    /// Retourne tous les objets du set (version asynchrone).
    ///
    /// # Returns
    ///
    /// Un vecteur contenant des clones des `Arc` pointant vers tous les objets du set.
    /// L'ordre des éléments n'est pas garanti.
    ///
    /// # Examples
    ///
    /// ```
    /// let set = UpnpObjectSet::new();
    /// 
    /// for obj in set.all().await {
    ///     println!("Objet: {}", obj.get_name());
    /// }
    /// ```
    pub async fn all(&self) -> Vec<Arc<T>> {
        let guard = self.objects.read().await;
        guard.values().cloned().collect()
    }

    /// Retourne tous les objets du set (version synchrone bloquante).
    ///
    /// Cette méthode bloque le thread appelant jusqu'à ce que le verrou de lecture
    /// soit obtenu. Utilisez cette version uniquement dans du code synchrone.
    /// Pour du code asynchrone, préférez [`all()`](Self::all).
    ///
    /// # Returns
    ///
    /// Un vecteur contenant des clones des `Arc` pointant vers tous les objets du set.
    /// L'ordre des éléments n'est pas garanti.
    ///
    /// # Examples
    ///
    /// ```
    /// let set = UpnpObjectSet::new();
    /// 
    /// // Dans un contexte synchrone
    /// for obj in set.get_all() {
    ///     println!("Objet: {}", obj.get_name());
    /// }
    /// ```
    ///
    /// # Avertissement
    ///
    /// N'appelez pas cette méthode depuis un contexte asynchrone car elle peut
    /// bloquer l'executor Tokio. Utilisez [`all()`](Self::all) à la place.
    pub fn get_all(&self) -> Vec<Arc<T>> {
        let guard = self.objects.blocking_read();
        guard.values().cloned().collect()
    }
}