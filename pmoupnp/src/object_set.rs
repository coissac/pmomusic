use std::{collections::HashMap, sync::Arc};

use std::sync::RwLock;

use crate::{UpnpDeepClone, UpnpObjectSet, UpnpObjectSetError, UpnpTypedObject};

/// Implémentation du clonage profond pour `UpnpObjectSet`.
///
/// Cette implémentation crée une copie complète et indépendante du set,
/// en clonant chaque objet `T` et en créant de nouveaux `Arc` autour de ces clones.
/// Les modifications sur l'un des sets n'affectent pas l'autre.
impl<T: UpnpTypedObject> UpnpDeepClone for UpnpObjectSet<T> {
    fn deep_clone(&self) -> Self {
        let guard = self.objects.read().unwrap();

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
        let guard = self.objects.read().unwrap();

        Self {
            objects: RwLock::new(guard.clone()),
        }
    }
}

impl<T: UpnpTypedObject> UpnpObjectSet<T> {
    /// Crée un nouveau `UpnpObjectSet` vide.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let set: UpnpObjectSet<MyObject> = UpnpObjectSet::new();
    /// ```
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
    /// ```ignore
    /// let mut set = UpnpObjectSet::new();
    /// let obj = Arc::new(MyObject::new("test"));
    /// set.insert(obj)?;
    /// ```
    pub fn insert(&mut self, object: Arc<T>) -> Result<(), UpnpObjectSetError> {
        let mut guard = self.objects.write().unwrap();
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
    /// ```ignore
    /// let mut set = UpnpObjectSet::new();
    /// let obj1 = Arc::new(MyObject::new("test"));
    /// let obj2 = Arc::new(MyObject::new("test")); // Même nom
    ///
    /// set.insert_or_replace(obj1);
    /// set.insert_or_replace(obj2); // Remplace obj1
    /// ```
    pub fn insert_or_replace(&mut self, object: Arc<T>) {
        let mut guard = self.objects.write().unwrap();
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
    /// ```ignore
    /// let set = UpnpObjectSet::new();
    /// let obj = Arc::new(MyObject::new("test"));
    ///
    /// if set.contains(obj.clone()) {
    ///     println!("L'objet existe déjà");
    /// }
    /// ```
    pub fn contains(&self, object: Arc<T>) -> bool {
        let guard = self.objects.read().unwrap();
        let key: String = object.get_name().to_string();

        guard.contains_key(&key)
    }

    /// Récupère un objet par son nom.
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
    /// ```ignore
    /// let set = UpnpObjectSet::new();
    ///
    /// if let Some(obj) = set.get_by_name("test") {
    ///     println!("Objet trouvé: {}", obj.get_name());
    /// }
    /// ```
    pub fn get_by_name(&self, name: &str) -> Option<Arc<T>> {
        let guard = self.objects.read().unwrap();
        guard.get(name).cloned()
    }

    /// Retourne tous les objets du set.
    ///
    /// # Returns
    ///
    /// Un vecteur contenant des clones des `Arc` pointant vers tous les objets du set.
    /// L'ordre des éléments n'est pas garanti.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let set = UpnpObjectSet::new();
    ///
    /// for obj in set.all() {
    ///     println!("Objet: {}", obj.get_name());
    /// }
    /// ```
    ///
    /// # Thread-safety
    ///
    /// Cette méthode acquiert un verrou de lecture. Plusieurs threads peuvent
    /// appeler cette méthode simultanément sans blocage.
    pub fn all(&self) -> Vec<Arc<T>> {
        let guard = self.objects.read().unwrap();
        guard.values().cloned().collect()
    }
}
