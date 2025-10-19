//! ## Hiérarchie des traits
//!
//! ```text
//! Clone + Debug
//!     └─> UpnpObject (trait de base)
//!         ├─> UpnpModel (modèles créant des instances)
//!         ├─> UpnpInstance (instances concrètes)
//!         ├─> UpnpTyped (objets avec nom et type)
//!         │   └─> UpnpTypedObject = UpnpObject + UpnpTyped
//!         │       └─> UpnpTypedInstance = UpnpTypedObject + UpnpInstance
//!         └─> UpnpSet (collections) + UpnpDeepClone
//!             ├─> UpnpModelSet = UpnpSet + UpnpModel
//!             └─> UpnInstanceSet = UpnpSet + UpnpInstance
//!
//! UpnpDeepClone (indépendant)
//! ```
//!
//! ## Description des traits
//!
//! - **Traits de base** :
//!   - [`UpnpObject`] : Trait principal avec sérialisation XML/Markdown
//!   - [`UpnpDeepClone`] : Clonage profond (indépendant de la hiérarchie)
//!
//! - **Traits de spécialisation niveau 1** :
//!   - [`UpnpModel`] : Modèle pouvant créer des instances
//!   - [`UpnpInstance`] : Instance concrète créée depuis un modèle
//!   - [`UpnpTyped`] : Ajoute les informations de type et nom
//!   - [`UpnpSet`] : Marque un objet comme collection
//!
//! - **Traits combinés niveau 2** :
//!   - [`UpnpTypedObject`] : Objet typé (marker trait)
//!
//! - **Traits combinés niveau 3** :
//!   - [`UpnpTypedInstance`] : Instance typée (marker trait)
//!   - [`UpnpModelSet`] : Collection de modèles (marker trait)
//!   - [`UpnInstanceSet`] : Collection d'instances (marker trait)

use std::{fmt::Debug, sync::Arc};

use xmltree::{Element, EmitterConfig};

use crate::UpnpObjectType;

/// Trait pour le clonage profond d'objets UPnP.
///
/// Contrairement au trait standard [`Clone`] qui peut effectuer un clonage superficiel
/// (partage via `Arc`), ce trait garantit un clonage complet et indépendant de l'objet.
///
/// # Note
///
/// Ce trait est indépendant de la hiérarchie [`UpnpObject`] et peut être implémenté
/// séparément.
pub trait UpnpDeepClone {
    /// Crée un clone profond de l'objet.
    ///
    /// Tous les éléments internes sont clonés, créant un objet complètement indépendant.
    fn deep_clone(&self) -> Self;
}

/// Trait de base pour tous les objets UPnP.
///
/// Ce trait fournit les fonctionnalités communes à tous les objets UPnP :
/// - Sérialisation XML
/// - Conversion en Markdown
/// - Identification du type d'objet (instance ou set)
///
/// # Traits requis
///
/// - [`Clone`] : Pour pouvoir dupliquer les objets
/// - [`Debug`] : Pour le débogage
///
/// # Hiérarchie
///
/// Ce trait est à la base de toute la hiérarchie UPnP. Voir la documentation du module
/// pour le graphe complet.
pub trait UpnpObject: Clone + Debug {
    /// Convertit l'objet en élément XML.
    ///
    /// # Returns
    ///
    /// Un [`Element`] xmltree représentant l'objet.
    fn to_xml_element(&self) -> Element;

    /// Convertit l'objet en chaîne XML formatée.
    ///
    /// Génère une représentation XML complète avec en-tête et indentation.
    ///
    /// # Returns
    ///
    /// Une chaîne XML formatée avec :
    /// - En-tête `<?xml version="1.0" encoding="UTF-8"?>`
    /// - Indentation de 2 espaces
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let xml = my_object.to_xml();
    /// println!("{}", xml);
    /// // <?xml version="1.0" encoding="UTF-8"?>
    /// // <element>
    /// //   <child>value</child>
    /// // </element>
    /// ```
    fn to_xml(&self) -> String {
        let elem = self.to_xml_element();

        let config = EmitterConfig::new()
            .perform_indent(true)
            .indent_string("  ");

        let mut buf = Vec::new();
        elem.write_with_config(&mut buf, config)
            .expect("Failed to write XML");

        String::from_utf8(buf).expect("Invalid UTF-8")
    }

    /// Convertit l'objet en représentation Markdown.
    ///
    /// Génère une vue hiérarchique de la structure XML en format Markdown,
    /// avec détection automatique des URLs et images.
    ///
    /// # Fonctionnalités
    ///
    /// - Les URLs sont converties en liens cliquables
    /// - Les URLs d'images sont affichées comme images
    /// - Les attributs sont formatés comme `key=value`
    /// - Structure hiérarchique avec indentation
    ///
    /// # Returns
    ///
    /// Une chaîne Markdown formatée.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let md = my_object.to_markdown();
    /// println!("{}", md);
    /// // # UPnP XML (Markdown view)
    /// //
    /// // - **element**
    /// //   - **child**: `value`
    /// ```
    fn to_markdown(&self) -> String {
        let elem = self.to_xml_element();
        let mut md = String::new();

        fn is_url(s: &str) -> bool {
            s.starts_with("http://") || s.starts_with("https://") || s.starts_with("urn:")
        }

        fn is_image_url(s: &str) -> bool {
            let s = s.to_lowercase();
            s.ends_with(".png")
                || s.ends_with(".jpg")
                || s.ends_with(".jpeg")
                || s.ends_with(".gif")
                || s.ends_with(".svg")
                || s.ends_with(".webp")
        }

        fn format_value(v: &str) -> String {
            let v = v.trim().to_string();
            if is_url(&v) {
                if is_image_url(&v) {
                    format!("[{}]({})<br>![]({})", v, v, v)
                } else {
                    format!("[{}]({})", v, v)
                }
            } else {
                format!("`{}`", v)
            }
        }

        fn recurse(elem: &xmltree::Element, md: &mut String, depth: usize) {
            let indent = "  ".repeat(depth);
            md.push_str(&format!("{}- **{}**", indent, elem.name));

            if !elem.attributes.is_empty() {
                let attrs: Vec<String> = elem
                    .attributes
                    .iter()
                    .map(|(k, v)| format!("{}={}", k, format_value(v)))
                    .collect();
                md.push_str(&format!(" ({})", attrs.join(", ")));
            }

            if let Some(text) = elem
                .get_text()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
            {
                md.push_str(&format!(": {}", format_value(&text)));
            }

            md.push('\n');

            for child in &elem.children {
                if let xmltree::XMLNode::Element(child_elem) = child {
                    recurse(child_elem, md, depth + 1);
                }
            }
        }

        md.push_str("# UPnP XML (Markdown view)\n\n");
        recurse(&elem, &mut md, 0);
        md
    }

    /// Indique si l'objet est une instance.
    ///
    /// # Returns
    ///
    /// `false` par défaut. Surchargé par [`UpnpInstance`] pour retourner `true`.
    fn is_instance(&self) -> bool {
        false
    }

    /// Indique si l'objet est une collection (set).
    ///
    /// # Returns
    ///
    /// `false` par défaut. Surchargé par [`UpnpSet`] pour retourner `true`.
    fn is_set(&self) -> bool {
        false
    }
}

/// Trait pour les modèles UPnP qui peuvent créer des instances.
///
/// Un modèle représente la définition ou template d'un objet UPnP, tandis qu'une
/// instance est une occurrence concrète de cet objet.
///
/// # Type associé
///
/// - [`Instance`](Self::Instance) : Le type d'instance créée par ce modèle
///
/// # Méthodes
///
/// - [`create_instance`](Self::create_instance) : Crée une nouvelle instance
///
/// # Hiérarchie
///
/// ```text
/// UpnpObject
///     └─> UpnpModel
/// ```
///
/// # Relation avec UpnpInstance
///
/// `UpnpModel` et [`UpnpInstance`] sont liés via leurs types associés :
/// - Le modèle spécifie quel type d'instance il crée
/// - L'instance spécifie de quel type de modèle elle provient
///
/// # Examples
///
/// ```ignore
/// struct DeviceModel { /* ... */ }
/// struct DeviceInstance { /* ... */ }
///
/// impl UpnpModel for DeviceModel {
///     type Instance = DeviceInstance;
/// }
///
/// impl UpnpInstance for DeviceInstance {
///     type Model = DeviceModel;
///     
///     fn new(model: &DeviceModel) -> Self {
///         // Création de l'instance depuis le modèle
///     }
/// }
///
/// // Utilisation
/// let model = DeviceModel::new();
/// let instance = model.create_instance(); // Arc<DeviceInstance>
/// ```
pub trait UpnpModel: UpnpObject {
    /// Le type d'instance créée par ce modèle.
    type Instance: UpnpInstance<Model = Self>;

    /// Crée une nouvelle instance à partir de ce modèle.
    ///
    /// # Returns
    ///
    /// Un `Arc` contenant la nouvelle instance créée.
    ///
    /// # Implémentation par défaut
    ///
    /// Par défaut, appelle [`UpnpInstance::new`] avec une référence vers ce modèle
    /// et encapsule le résultat dans un `Arc`.
    fn create_instance(&self) -> Arc<Self::Instance> {
        Arc::new(Self::Instance::new(self))
    }
}

/// Trait pour les instances UPnP concrètes.
///
/// Une instance représente une occurrence concrète d'un objet UPnP, créée à partir
/// d'un modèle ([`UpnpModel`]).
///
/// # Type associé
///
/// - [`Model`](Self::Model) : Le type du modèle dont cette instance dérive
///
/// # Méthodes requises
///
/// - [`new`](Self::new) : Constructeur créant l'instance depuis un modèle
///
/// # Hiérarchie
///
/// ```text
/// UpnpObject
///     └─> UpnpInstance
/// ```
///
/// # Relation avec UpnpModel
///
/// Voir la documentation de [`UpnpModel`] pour comprendre la relation entre
/// modèles et instances.
pub trait UpnpInstance: UpnpObject {
    /// Le type du modèle dont cette instance est dérivée.
    type Model: UpnpModel<Instance = Self>;

    /// Crée une nouvelle instance à partir d'un modèle.
    ///
    /// # Arguments
    ///
    /// * `model` - Référence vers le modèle à partir duquel créer l'instance
    ///
    /// # Returns
    ///
    /// Une nouvelle instance initialisée depuis le modèle.
    fn new(model: &Self::Model) -> Self;

    /// Indique que cet objet est une instance.
    ///
    /// # Returns
    ///
    /// Toujours `true` pour les instances.
    fn is_instance(&self) -> bool {
        true
    }
}

/// Trait pour les objets UPnP typés.
///
/// Ajoute les informations de type et de nom aux objets UPnP.
///
/// # Méthodes requises
///
/// - [`as_upnp_object_type`](Self::as_upnp_object_type) : Accès au type de l'objet
///
/// # Méthodes fournies
///
/// - [`get_name`](Self::get_name) : Récupère le nom de l'objet
/// - [`get_object_type`](Self::get_object_type) : Récupère le type de l'objet
///
/// # Hiérarchie
///
/// ```text
/// UpnpObject
///     └─> UpnpTyped
/// ```
pub trait UpnpTyped: UpnpObject {
    /// Retourne une référence vers le type de l'objet.
    fn as_upnp_object_type(&self) -> &UpnpObjectType;

    /// Retourne le nom de l'objet.
    ///
    /// # Returns
    ///
    /// Une référence vers le nom de l'objet.
    fn get_name(&self) -> &String {
        &self.as_upnp_object_type().name
    }

    /// Retourne le type de l'objet sous forme de chaîne.
    ///
    /// # Returns
    ///
    /// Une référence vers le type de l'objet (ex: "Device", "Service", etc.).
    fn get_object_type(&self) -> &String {
        &self.as_upnp_object_type().object_type
    }
}

/// Trait marqueur pour les objets UPnP typés.
///
/// Combine [`UpnpObject`] et [`UpnpTyped`] pour créer un objet avec toutes
/// les fonctionnalités de base plus les informations de type.
///
/// # Hiérarchie
///
/// ```text
/// UpnpObject + UpnpTyped
///     └─> UpnpTypedObject
/// ```
///
/// # Note
///
/// C'est un *marker trait* (trait marqueur) sans méthodes supplémentaires.
pub trait UpnpTypedObject: UpnpObject + UpnpTyped {}

/// Trait marqueur pour les instances typées UPnP.
///
/// Combine [`UpnpTypedObject`] et [`UpnpInstance`] pour représenter une instance
/// concrète d'un objet typé avec toutes les fonctionnalités :
/// - Sérialisation XML/Markdown (de [`UpnpObject`])
/// - Informations de type et nom (de [`UpnpTyped`])
/// - Relation avec un modèle (de [`UpnpInstance`])
///
/// # Hiérarchie
///
/// ```text
/// UpnpTypedObject + UpnpInstance
///     └─> UpnpTypedInstance
/// ```
///
/// # Note
///
/// Ce trait ajoute la méthode [`get_model`](Self::get_model) pour accéder
/// au modèle de l'instance. Les collections d'instances ([`UpnInstanceSet`])
/// n'ont pas cette méthode car elles contiennent plusieurs instances.
pub trait UpnpTypedInstance: UpnpTypedObject + UpnpInstance
where
    Self::Model: UpnpModel<Instance = Self>,
{
    /// Retourne une référence vers le modèle dont cette instance est dérivée.
    ///
    /// Permet d'accéder aux métadonnées et contraintes définies dans le modèle,
    /// telles que les plages de valeurs autorisées, les types, les descriptions, etc.
    ///
    /// # Returns
    ///
    /// Une référence immuable vers le modèle.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let instance = model.create_instance();
    ///
    /// // Accéder aux propriétés du modèle depuis l'instance
    /// let model_ref = instance.get_model();
    /// println!("Instance du modèle: {}", model_ref.get_name());
    ///
    /// // Vérifier les contraintes définies dans le modèle
    /// if let Some(range) = model_ref.get_range() {
    ///     println!("Plage autorisée: {:?}", range);
    /// }
    /// ```
    ///
    /// # Use cases
    ///
    /// Cette méthode est particulièrement utile pour :
    /// - Valider des valeurs contre les contraintes du modèle
    /// - Accéder aux métadonnées sans dupliquer les informations
    /// - Afficher des informations de type ou de description
    /// - Implémenter des logiques conditionnelles basées sur le modèle
    ///
    /// # Différence avec les traits spécifiques
    ///
    /// Pour les variables d'état, le trait [`UpnpVariable`](crate::state_variables::UpnpVariable)
    /// fournit également `get_definition()` qui est sémantiquement équivalent
    /// mais spécifique au domaine des variables.
    fn get_model(&self) -> &Self::Model;
}

/// Trait marqueur pour les collections UPnP.
///
/// Représente un ensemble (set) d'objets UPnP.
///
/// # Super-traits requis
///
/// - [`UpnpObject`] : Fonctionnalités de base (XML, etc.)
/// - [`UpnpDeepClone`] : Permet le clonage profond des collections
///
/// # Implémentation
///
/// Ce trait surcharge [`UpnpObject::is_set`] pour retourner `true`.
///
/// # Hiérarchie
///
/// ```text
/// UpnpObject + UpnpDeepClone
///     └─> UpnpSet
/// ```
///
/// # Note sur le clonage
///
/// Les collections UPnP contiennent généralement des `Arc<T>` vers leurs éléments.
/// Le trait [`Clone`] (via `UpnpObject`) effectue un clonage shallow des `Arc`,
/// tandis que [`UpnpDeepClone`] clone profondément les éléments contenus.
///
/// # Examples
///
/// ```ignore
/// struct ServiceSet {
///     services: HashMap<String, Arc<Service>>,
/// }
///
/// impl Clone for ServiceSet {
///     fn clone(&self) -> Self {
///         // Clone shallow : partage les Services via Arc
///         Self {
///             services: self.services.clone()
///         }
///     }
/// }
///
/// impl UpnpDeepClone for ServiceSet {
///     fn deep_clone(&self) -> Self {
///         // Clone profond : crée de nouveaux Services
///         let deep_services = self.services
///             .iter()
///             .map(|(k, v)| (k.clone(), Arc::new((**v).clone())))
///             .collect();
///         
///         Self {
///             services: deep_services
///         }
///     }
/// }
/// ```
pub trait UpnpSet: UpnpObject + UpnpDeepClone {
    /// Indique que cet objet est une collection.
    ///
    /// # Returns
    ///
    /// Toujours `true` pour les collections.
    fn is_set(&self) -> bool {
        true
    }
}

/// Trait marqueur pour les collections de modèles UPnP.
///
/// Combine [`UpnpSet`] et [`UpnpModel`] pour représenter une collection
/// de modèles qui peut elle-même créer une collection d'instances.
///
/// # Cas d'usage
///
/// Ce trait est utilisé quand une collection de modèles doit pouvoir instancier
/// une collection d'instances correspondante. Par exemple :
/// - Un ensemble de modèles de services d'un device qui crée un ensemble d'instances de services
/// - Une liste de modèles d'actions qui instancie une liste d'actions actives
/// - Une collection de modèles de variables d'état qui génère une collection d'instances
///
/// # Hiérarchie
///
/// ```text
/// UpnpSet + UpnpModel
///     └─> UpnpModelSet
/// ```
///
/// # Relation avec d'autres traits
///
/// - [`UpnpSet`] : Fournit les fonctionnalités de collection
/// - [`UpnpModel`] : Fournit la capacité de créer des instances
/// - [`UpnInstanceSet`] : Représente les collections d'instances (contrepartie)
///
/// # Note
///
/// C'est un *marker trait* (trait marqueur) sans méthodes supplémentaires.
/// Il est automatiquement implémenté pour tous les types éligibles via une
/// blanket implementation.
///
/// # Examples
///
/// ```ignore
/// /// Collection de modèles de services
/// struct ServiceSetModel {
///     services: Vec<Arc<ServiceModel>>,
/// }
///
/// /// Collection d'instances de services
/// struct ServiceSetInstance {
///     model: Arc<ServiceSetModel>,
///     service_instances: Vec<Arc<ServiceInstance>>,
/// }
///
/// impl UpnpObject for ServiceSetModel { /* ... */ }
/// impl UpnpSet for ServiceSetModel {}
///
/// impl UpnpModel for ServiceSetModel {
///     type Instance = ServiceSetInstance;
///     
///     fn create_instance(&self) -> Arc<ServiceSetInstance> {
///         // Créer des instances pour chaque service
///         let instances = self.services
///             .iter()
///             .map(|model| model.create_instance())
///             .collect();
///         
///         Arc::new(ServiceSetInstance {
///             model: Arc::new(self.clone()),
///             service_instances: instances,
///         })
///     }
/// }
///
/// // UpnpModelSet est automatiquement implémenté !
///
/// // Utilisation
/// let model_set = ServiceSetModel::new();
/// let instance_set = model_set.create_instance(); // Crée toutes les instances
/// ```
pub trait UpnpModelSet: UpnpSet + UpnpModel {}

/// Trait marqueur pour les collections d'instances UPnP.
///
/// Combine [`UpnpSet`] et [`UpnpInstance`] pour représenter une collection
/// d'instances UPnP. Cela permet d'avoir des collections qui sont elles-mêmes
/// des instances créées depuis un modèle.
///
/// # Hiérarchie
///
/// ```text
/// UpnpSet + UpnpInstance
///     └─> UpnInstanceSet
/// ```
///
/// # Note
///
/// C'est un *marker trait* (trait marqueur) sans méthodes supplémentaires.
pub trait UpnInstanceSet: UpnpSet + UpnpInstance {}

/// Implémentation automatique de [`UpnInstanceSet`] pour tous les types éligibles.
///
/// Cette *blanket implementation* fournit automatiquement le trait [`UpnInstanceSet`]
/// à tout type `T` qui implémente à la fois [`UpnpSet`] et [`UpnpInstance`].
///
/// # Contraintes
///
/// - `T` doit implémenter [`UpnpSet`] (collection d'objets UPnP)
/// - `T` doit implémenter [`UpnpInstance`] (instance créée depuis un modèle)
///
/// # Pourquoi cette implémentation existe
///
/// Certaines collections UPnP sont elles-mêmes des instances (par exemple, une
/// collection de services pour un device spécifique). Ce trait marker permet
/// d'identifier ces collections qui combinent les deux aspects. La blanket
/// implementation évite d'avoir à l'implémenter manuellement pour chaque type.
///
/// # Utilisation
///
/// ```ignore
/// struct ServiceSetInstance {
///     model: Arc<ServiceSetModel>,
///     services: Vec<Arc<ServiceInstance>>,
/// }
///
/// impl UpnpObject for ServiceSetInstance { /* ... */ }
/// impl UpnpSet for ServiceSetInstance {}
/// impl UpnpInstance for ServiceSetInstance {
///     type Model = ServiceSetModel;
///     fn new(model: &ServiceSetModel) -> Self { /* ... */ }
/// }
///
/// // UpnInstanceSet est automatiquement implémenté !
///
/// fn process_instance_set<T: UpnInstanceSet>(set: &T) {
///     if set.is_set() && set.is_instance() {
///         println!("C'est une collection ET une instance");
///     }
/// }
/// ```
impl<T> UpnInstanceSet for T where T: UpnpSet + UpnpInstance {}

/// Implémentation automatique de [`UpnpTypedObject`] pour tous les types éligibles.
///
/// Cette *blanket implementation* fournit automatiquement le trait [`UpnpTypedObject`]
/// à tout type `T` qui implémente à la fois [`UpnpObject`] et [`UpnpTyped`].
///
/// # Contraintes
///
/// - `T` doit implémenter [`UpnpObject`] (fonctionnalités de base UPnP)
/// - `T` doit implémenter [`UpnpTyped`] (informations de type et nom)
///
/// # Utilisation
///
/// ```ignore
/// struct Device {
///     object_type: UpnpObjectType,
/// }
///
/// impl UpnpObject for Device { /* ... */ }
/// impl UpnpTyped for Device { /* ... */ }
///
/// // UpnpTypedObject est automatiquement implémenté !
/// fn process<T: UpnpTypedObject>(obj: &T) {
///     println!("{}", obj.get_name());
/// }
/// ```
impl<T> UpnpTypedObject for T where T: UpnpObject + UpnpTyped {}

/// Implémentation automatique de [`UpnpModelSet`] pour tous les types éligibles.
///
/// Cette *blanket implementation* fournit automatiquement le trait [`UpnpModelSet`]
/// à tout type `T` qui implémente à la fois [`UpnpSet`] et [`UpnpModel`].
///
/// # Contraintes
///
/// - `T` doit implémenter [`UpnpSet`] (collection d'objets UPnP)
/// - `T` doit implémenter [`UpnpModel`] (peut créer des instances)
///
/// # Pourquoi cette implémentation existe
///
/// [`UpnpModelSet`] est un *marker trait* qui identifie les collections pouvant
/// créer des collections d'instances. Plutôt que de demander aux développeurs
/// d'écrire manuellement `impl UpnpModelSet for MyType {}`, cette blanket
/// implementation le fait automatiquement dès que les traits requis sont implémentés.
///
/// # Fonctionnement
///
/// Lorsque vous définissez une collection de modèles :
///
/// ```ignore
/// struct ActionSetModel {
///     actions: Vec<Arc<ActionModel>>,
/// }
///
/// impl UpnpObject for ActionSetModel { /* ... */ }
/// impl UpnpSet for ActionSetModel {}
///
/// impl UpnpModel for ActionSetModel {
///     type Instance = ActionSetInstance;
///     fn create_instance(&self) -> Arc<ActionSetInstance> { /* ... */ }
/// }
/// ```
///
/// Le compilateur Rust vérifie automatiquement que `ActionSetModel` satisfait
/// toutes les contraintes (implémente `UpnpSet` ET `UpnpModel`) et applique
/// donc `UpnpModelSet` sans code supplémentaire.
///
/// # Utilisation dans des signatures génériques
///
/// ```ignore
/// fn process_model_set<T: UpnpModelSet>(set: &T) {
///     println!("Processing model set that can create instances");
///     let instance = set.create_instance();
///     // ...
/// }
/// ```
///
/// # Différence avec UpnInstanceSet
///
/// - [`UpnpModelSet`] : Collection de **modèles** (peut créer des instances)
/// - [`UpnInstanceSet`] : Collection d'**instances** (créée depuis un modèle)
impl<T> UpnpModelSet for T where T: UpnpSet + UpnpModel {}
