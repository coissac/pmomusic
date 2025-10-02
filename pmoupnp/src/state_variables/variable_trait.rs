use crate::{
    state_variables::StateVariable,
    variable_types::{StateValue, UpnpVarType},
};

/// Trait pour accéder aux propriétés et contraintes d'une variable UPnP.
///
/// Ce trait fournit une interface uniforme pour interroger les métadonnées,
/// contraintes et comportements d'une variable UPnP, qu'il s'agisse d'une
/// définition ([`StateVariable`]) ou d'une instance ([`StateVarInstance`]).
///
/// # Architecture
///
/// Le trait utilise le pattern "trait avec implémentation par défaut" :
/// - Une seule méthode requise : [`get_definition`](Self::get_definition)
/// - Toutes les autres méthodes sont implémentées par défaut en déléguant à la définition
///
/// Cela permet une interface cohérente entre modèles et instances sans duplication de code.
///
/// # Hiérarchie
///
/// ```text
/// UpnpVariable
///     ├─> StateVariable (get_definition() retourne self)
///     └─> StateVarInstance (get_definition() retourne self.definition)
/// ```
///
/// # Examples
///
/// ```ignore
/// fn display_variable_info<V: UpnpVariable>(var: &V) {
///     println!("Variable: {}", var.get_definition().get_name());
///     
///     if var.has_default() {
///         println!("Default: {:?}", var.get_default());
///     }
///     
///     if var.has_range() {
///         println!("Has range constraints");
///     }
///     
///     if var.has_allowed_values() {
///         println!("Has allowed values list");
///     }
/// }
/// ```
pub trait UpnpVariable {
    /// Retourne une référence vers la définition de la variable.
    ///
    /// Cette méthode est la base de toutes les autres méthodes du trait.
    ///
    /// # Implementation
    ///
    /// - Pour [`StateVariable`] : retourne `self`
    /// - Pour [`StateVarInstance`] : retourne `self.definition`
    fn get_definition(&self) -> &StateVariable;

    /// Indique si la variable a un pas (step) défini.
    ///
    /// Le pas définit l'incrément minimal entre deux valeurs valides pour
    /// les types numériques.
    ///
    /// # Returns
    ///
    /// `true` si un pas est défini, `false` sinon.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// if var.has_step() {
    ///     println!("Step: {:?}", var.get_step());
    /// }
    /// ```
    fn has_step(&self) -> bool {
        self.get_definition().step.is_some()
    }

    /// Retourne le pas (step) de la variable s'il est défini.
    ///
    /// # Returns
    ///
    /// - `Some(StateValue)` si un pas est défini
    /// - `None` sinon
    ///
    /// # See also
    ///
    /// - [`has_step`](Self::has_step) pour tester l'existence
    fn get_step(&self) -> Option<StateValue> {
        self.get_definition().step.clone()
    }

    /// Indique si la variable a une plage de valeurs (range) définie.
    ///
    /// La plage définit les valeurs minimale et maximale acceptables.
    ///
    /// # Returns
    ///
    /// `true` si une plage est définie, `false` sinon.
    fn has_range(&self) -> bool {
        self.get_definition().value_range.is_some()
    }

    /// Indique si la variable est modifiable.
    ///
    /// Une variable non modifiable est en lecture seule.
    ///
    /// # Returns
    ///
    /// `true` si la variable peut être modifiée, `false` sinon.
    fn is_modifiable(&self) -> bool {
        self.get_definition().modifiable
    }

    /// Indique si la variable a des conditions d'événement définies.
    ///
    /// Les conditions d'événement déterminent quand des notifications
    /// doivent être envoyées lors de changements de valeur.
    ///
    /// # Returns
    ///
    /// `true` si au moins une condition d'événement existe, `false` sinon.
    ///
    /// # Note
    ///
    /// Retourne `false` si le lock est empoisonné (poisoned).
    fn has_event_conditions(&self) -> bool {
        let guard = self.get_definition().event_conditions.blocking_read();
        !guard.is_empty()
    }

    /// Vérifie si une condition d'événement spécifique existe.
    ///
    /// # Arguments
    ///
    /// * `name` - Le nom de la condition à rechercher
    ///
    /// # Returns
    ///
    /// `true` si la condition existe, `false` sinon.
    ///
    /// # Note
    ///
    /// Retourne `false` si le lock est empoisonné (poisoned).
    fn has_event_condition(&self, name: &String) -> bool {
        let guard = self.get_definition().event_conditions.blocking_read();
        guard.contains_key(name)
    }

    /// Indique si la variable a une description non vide.
    ///
    /// # Returns
    ///
    /// `true` si une description existe et n'est pas vide, `false` sinon.
    fn has_description(&self) -> bool {
        !self.get_definition().description.is_empty()
    }

    /// Retourne la description de la variable.
    ///
    /// # Returns
    ///
    /// La description sous forme de `String`. Peut être vide.
    ///
    /// # See also
    ///
    /// - [`has_description`](Self::has_description) pour tester si non vide
    fn get_description(&self) -> String {
        self.get_definition().description.clone()
    }

    /// Indique si la variable a une valeur par défaut définie explicitement.
    ///
    /// # Returns
    ///
    /// `true` si une valeur par défaut est explicitement définie, `false` sinon.
    ///
    /// # Note
    ///
    /// Même si cette méthode retourne `false`, [`get_default`](Self::get_default)
    /// retournera toujours une valeur (la valeur par défaut du type).
    fn has_default(&self) -> bool {
        self.get_definition().default_value.is_some()
    }

    /// Retourne la valeur par défaut de la variable.
    ///
    /// # Returns
    ///
    /// La valeur par défaut. Si aucune valeur par défaut n'est explicitement
    /// définie, retourne la valeur par défaut du type de la variable
    /// (ex: 0 pour les entiers, chaîne vide pour String, etc.).
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let default = var.get_default();
    /// println!("Default value: {:?}", default);
    /// ```
    fn get_default(&self) -> StateValue {
        self.get_definition()
            .default_value
            .clone()
            .unwrap_or_else(|| self.get_definition().as_state_var_type().default_value())
    }

    /// Indique si la variable a une liste de valeurs autorisées.
    ///
    /// Lorsqu'une liste de valeurs autorisées est définie, seules ces valeurs
    /// sont acceptables pour la variable.
    ///
    /// # Returns
    ///
    /// `true` si une liste non vide de valeurs autorisées existe, `false` sinon.
    ///
    /// # Note
    ///
    /// Retourne `false` si le lock est empoisonné (poisoned).
    fn has_allowed_values(&self) -> bool {
        let guard = self.get_definition()
            .allowed_values
            .blocking_read();

        !guard.is_empty()
    }

    /// Vérifie si une valeur fait partie des valeurs autorisées.
    ///
    /// # Arguments
    ///
    /// * `value` - La valeur à vérifier
    ///
    /// # Returns
    ///
    /// `true` si la valeur est dans la liste des valeurs autorisées, `false` sinon.
    /// Retourne également `false` si aucune liste de valeurs autorisées n'est définie
    /// ou si le lock est empoisonné.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let value = StateValue::String("ON".to_string());
    /// if var.is_an_allowed_value(&value) {
    ///     println!("Value is allowed");
    /// }
    /// ```
    ///
    /// # Note
    ///
    /// Si aucune liste de valeurs autorisées n'est définie, cette méthode
    /// retourne `false`. Utilisez [`has_allowed_values`](Self::has_allowed_values)
    /// pour distinguer "pas de liste" de "valeur non autorisée".
    fn is_an_allowed_value(&self, value: &StateValue) -> bool {
        let guard = self.get_definition()
            .allowed_values
            .blocking_read();

        guard.contains(value)
    }

    /// Indique si la variable envoie des notifications d'événement.
    ///
    /// Les notifications d'événement sont envoyées aux abonnés lorsque
    /// la valeur de la variable change.
    ///
    /// # Returns
    ///
    /// `true` si les notifications sont activées, `false` sinon.
    ///
    /// # See also
    ///
    /// - [`has_event_conditions`](Self::has_event_conditions) pour vérifier
    ///   les conditions d'envoi d'événements
    fn is_sending_notification(&self) -> bool {
        self.get_definition().send_events
    }

    /// Indique si la variable a un parser de valeur personnalisé.
    ///
    /// Un parser personnalisé est utilisé pour convertir des chaînes de
    /// caractères en valeurs typées. Disponible uniquement pour les variables
    /// de type String.
    ///
    /// # Returns
    ///
    /// `true` si un parser est défini, `false` sinon.
    fn has_value_parser(&self) -> bool {
        self.get_definition().parse.is_some()
    }

    /// Indique si la variable a un marshaler de valeur personnalisé.
    ///
    /// Un marshaler personnalisé est utilisé pour sérialiser des valeurs
    /// en chaînes de caractères. Disponible uniquement pour les variables
    /// de type String.
    ///
    /// # Returns
    ///
    /// `true` si un marshaler est défini, `false` sinon.
    fn has_value_marshaler(&self) -> bool {
        self.get_definition().marshal.is_some()
    }
}
