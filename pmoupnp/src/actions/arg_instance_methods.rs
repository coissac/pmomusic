use std::{collections::HashMap, sync::{Arc, RwLock}};

use xmltree::Element;

use crate::{actions::{ActionInstanceSet, ActionSet, Argument, ArgumentInstance}, state_variables::StateVarInstance, UpnpInstance, UpnpObject, UpnpObjectType, UpnpTyped, UpnpTypedInstance};


impl UpnpObject for ArgumentInstance {
    fn to_xml_element(&self) -> Element {
        self.get_model().to_xml_element()
    }
}

impl UpnpTyped for ArgumentInstance {
    fn as_upnp_object_type(&self) -> &UpnpObjectType {
        return &self.object;
    }
}

/// Implémentation de [`UpnpTypedInstance`] pour [`ArgumentInstance`].
///
/// Cette implémentation permet d'accéder au modèle [`Argument`] depuis l'instance
/// via la méthode [`get_model()`](UpnpTypedInstance::get_model).
///
/// # Examples
///
/// ```ignore
/// use pmoupnp::UpnpTypedInstance;
///
/// let arg_instance = ArgumentInstance::new(&arg_model);
///
/// // Accéder au modèle
/// let model = arg_instance.get_model();
/// println!("Direction: in={}, out={}", model.is_in(), model.is_out());
/// println!("Related variable: {}", model.state_variable().get_name());
/// ```
impl UpnpTypedInstance for ArgumentInstance {
    /// Retourne une référence vers le modèle [`Argument`].
    ///
    /// Permet d'accéder aux métadonnées statiques définies dans le modèle :
    /// - Direction de l'argument (in/out)
    /// - Variable d'état associée
    /// - Nom et type
    fn get_model(&self) -> &Self::Model {
        &self.model
    }
}


/// Implémentation de [`UpnpInstance`] pour [`ArgumentInstance`].
///
/// Cette implémentation fournit le constructeur standard qui crée une instance
/// **non liée** d'un argument. La liaison à une [`StateVarInstance`] doit être
/// effectuée séparément via [`bind_variable`](ArgumentInstance::bind_variable).
///
/// # Processus de construction en deux phases
///
/// ```text
/// Phase 1 (new)           Phase 2 (bind_variable)
/// ┌─────────────────┐     ┌──────────────────────┐
/// │ ArgumentInstance│     │  StateVarInstance    │
/// │                 │     │                      │
/// │ model: Arc<...> │────>│  Liaison établie     │
/// │ variable: None  │     │  variable: Some(...) │
/// └─────────────────┘     └──────────────────────┘
///       ↓                            ↓
///   Création               bind_variable(&var)
/// ```
///
/// # Pourquoi deux phases ?
///
/// 1. **Ordre de création** : Les modèles (`Argument`) existent avant les instances
/// 2. **Validation différée** : Les dépendances sont vérifiées après instanciation
/// 3. **Découplage** : Permet de créer des arguments même si les variables n'existent pas encore
///
/// # Examples
///
/// ```ignore
/// use pmoupnp::actions::{Argument, ArgumentInstance};
/// use pmoupnp::UpnpInstance;
///
/// let arg_model = Argument::new_in("InstanceID".to_string(), instance_id_var);
/// 
/// // Création de l'instance - Phase 1
/// let arg_instance = ArgumentInstance::new(&arg_model);
/// 
/// // À ce stade, l'instance existe mais n'est pas encore liée
/// assert_eq!(arg_instance.get_name(), "InstanceID");
/// assert!(arg_instance.get_variable_instance().is_none());
/// 
/// // La liaison se fera plus tard via bind_variable()
/// ```
impl UpnpInstance for ArgumentInstance {
    type Model = Argument;

    /// Crée une nouvelle instance d'argument depuis son modèle.
    ///
    /// # Arguments
    ///
    /// * `from` - Référence vers le modèle [`Argument`] définissant cet argument
    ///
    /// # Returns
    ///
    /// Une nouvelle `ArgumentInstance` avec :
    /// - Nom copié depuis le modèle
    /// - Référence vers le modèle (clone)
    /// - `variable_instance` initialisé à `None` (liaison non établie)
    ///
    /// # État initial
    ///
    /// L'instance créée n'est **pas encore liée** à une variable d'état.
    /// Pour établir la liaison, appelez [`bind_variable`](ArgumentInstance::bind_variable).
    ///
    /// # Thread-safety
    ///
    /// L'instance retournée est thread-safe et peut être partagée via `Arc`.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use pmoupnp::UpnpInstance;
    /// 
    /// // Création depuis un modèle
    /// let instance = ArgumentInstance::new(&arg_model);
    /// 
    /// // L'instance hérite des propriétés du modèle
    /// assert_eq!(instance.get_name(), arg_model.get_name());
    /// assert_eq!(instance.is_in(), arg_model.is_in());
    /// 
    /// // Mais n'a pas encore de valeur runtime
    /// assert!(instance.get_variable_instance().is_none());
    /// ```
    fn new(from: &Argument) -> Self {
        Self {
            // Copie des métadonnées depuis le modèle
            object: UpnpObjectType {
                name: from.get_name().clone(),
                object_type: "ArgumentInstance".to_string(),
            },

            // Clone du modèle pour référence future
            model: from.clone(),

            // Initialisation à None - sera lié plus tard via bind_variable()
            // Arc<RwLock<...>> permet la modification thread-safe post-construction
            variable_instance: Arc::new(RwLock::new(None)),
        }
    }
}

// ============================================================================
// Méthodes de liaison et d'accès
// ============================================================================

impl ArgumentInstance {
    /// Lie cet argument à une instance de variable d'état.
    ///
    /// Cette méthode établit la connexion entre l'argument et sa variable d'état,
    /// permettant l'accès aux valeurs runtime lors de l'exécution d'actions.
    ///
    /// # Arguments
    ///
    /// * `var_instance` - Instance de la variable d'état à lier
    ///
    /// # Thread-safety
    ///
    /// Cette méthode acquiert un **write lock** sur `variable_instance` et peut
    /// bloquer si d'autres threads lisent actuellement la valeur.
    ///
    /// # Panics
    ///
    /// Panique si le lock est empoisonné (poisoned), ce qui ne devrait jamais
    /// arriver dans un usage normal.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use std::sync::Arc;
    /// 
    /// let arg_instance = ArgumentInstance::new(&arg_model);
    /// let var_instance = Arc::new(StateVarInstance::new(&state_var));
    /// 
    /// // Établir la liaison
    /// arg_instance.bind_variable(var_instance.clone());
    /// 
    /// // Vérifier que la liaison est établie
    /// assert!(arg_instance.get_variable_instance().is_some());
    /// ```
    ///
    /// # Note
    ///
    /// Cette méthode peut être appelée plusieurs fois pour changer la variable liée,
    /// bien que ce ne soit généralement pas recommandé dans un usage normal.
    pub fn bind_variable(&self, var_instance: Arc<StateVarInstance>) {
        let mut var = self.variable_instance.write().unwrap();
        *var = Some(var_instance);
    }

    /// Retourne l'instance de variable d'état liée, si elle existe.
    ///
    /// # Returns
    ///
    /// - `Some(Arc<StateVarInstance>)` si une variable est liée
    /// - `None` si aucune liaison n'a été établie via [`bind_variable`](Self::bind_variable)
    ///
    /// # Thread-safety
    ///
    /// Cette méthode acquiert un **read lock** sur `variable_instance`.
    /// Plusieurs threads peuvent lire simultanément sans blocage.
    ///
    /// # Panics
    ///
    /// Panique si le lock est empoisonné (poisoned).
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // Vérifier si la liaison existe
    /// if let Some(var) = arg_instance.get_variable_instance() {
    ///     println!("Variable liée : {}", var.get_name());
    ///     println!("Valeur actuelle : {}", var.value());
    /// } else {
    ///     println!("Aucune variable liée");
    /// }
    /// ```
    ///
    /// # Usage dans l'exécution d'actions
    ///
    /// ```ignore
    /// async fn execute_action(action: &ActionInstance) -> Result<(), ActionError> {
    ///     for arg in action.arguments_set().all() {
    ///         if let Some(var) = arg.get_variable_instance() {
    ///             // Utiliser var.value() pour lire/écrire
    ///             println!("Paramètre {} = {}", arg.get_name(), var.value());
    ///         } else {
    ///             return Err(ActionError::UnboundArgument(arg.get_name().to_string()));
    ///         }
    ///     }
    ///     Ok(())
    /// }
    /// ```
    pub fn get_variable_instance(&self) -> Option<Arc<StateVarInstance>> {
        self.variable_instance.read().unwrap().clone()
    }
}


impl UpnpInstance for ActionInstanceSet {
    type Model = ActionSet;
    
    fn new(_: &ActionSet) -> Self {
        Self { 
            objects: RwLock::new(HashMap::new()) 
        }
    }
}