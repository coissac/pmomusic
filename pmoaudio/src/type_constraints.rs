//! Système de contraintes de types pour les nodes audio
//!
//! Ce module définit les types et structures permettant de vérifier la compatibilité
//! entre les producers et consumers de chunks audio dans un pipeline.

use crate::AudioChunk;
use std::fmt;

/// Type d'échantillon supporté
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SampleType {
    /// Entier 16-bit
    I16,
    /// Entier 24-bit (I24)
    I24,
    /// Entier 32-bit
    I32,
    /// Flottant 32-bit
    F32,
    /// Flottant 64-bit
    F64,
}

impl SampleType {
    /// Vérifie si le type est un entier
    pub fn is_integer(&self) -> bool {
        matches!(self, SampleType::I16 | SampleType::I24 | SampleType::I32)
    }

    /// Vérifie si le type est un flottant
    pub fn is_float(&self) -> bool {
        matches!(self, SampleType::F32 | SampleType::F64)
    }

    /// Retourne la profondeur de bit
    pub fn bit_depth(&self) -> u8 {
        match self {
            SampleType::I16 => 16,
            SampleType::I24 => 24,
            SampleType::I32 | SampleType::F32 => 32,
            SampleType::F64 => 64,
        }
    }

    /// Extrait le type d'un AudioChunk
    pub fn from_audio_chunk(chunk: &AudioChunk) -> Self {
        match chunk {
            AudioChunk::I16(_) => SampleType::I16,
            AudioChunk::I24(_) => SampleType::I24,
            AudioChunk::I32(_) => SampleType::I32,
            AudioChunk::F32(_) => SampleType::F32,
            AudioChunk::F64(_) => SampleType::F64,
        }
    }
}

impl fmt::Display for SampleType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SampleType::I16 => write!(f, "I16"),
            SampleType::I24 => write!(f, "I24"),
            SampleType::I32 => write!(f, "I32"),
            SampleType::F32 => write!(f, "F32"),
            SampleType::F64 => write!(f, "F64"),
        }
    }
}

/// Catégorie de type acceptée
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeCategory {
    /// N'importe quel type entier (I16, I24, I32)
    AnyInteger,
    /// N'importe quel type flottant (F32, F64)
    AnyFloat,
    /// Un type spécifique uniquement
    Specific(SampleType),
    /// N'importe quel type (entier ou flottant)
    Any,
}

impl TypeCategory {
    /// Vérifie si cette catégorie accepte le type donné
    pub fn accepts(&self, sample_type: SampleType) -> bool {
        match self {
            TypeCategory::AnyInteger => sample_type.is_integer(),
            TypeCategory::AnyFloat => sample_type.is_float(),
            TypeCategory::Specific(t) => *t == sample_type,
            TypeCategory::Any => true,
        }
    }

    /// Retourne tous les types possibles pour cette catégorie
    pub fn possible_types(&self) -> Vec<SampleType> {
        match self {
            TypeCategory::AnyInteger => vec![SampleType::I16, SampleType::I24, SampleType::I32],
            TypeCategory::AnyFloat => vec![SampleType::F32, SampleType::F64],
            TypeCategory::Specific(t) => vec![*t],
            TypeCategory::Any => vec![
                SampleType::I16,
                SampleType::I24,
                SampleType::I32,
                SampleType::F32,
                SampleType::F64,
            ],
        }
    }
}

impl fmt::Display for TypeCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeCategory::AnyInteger => write!(f, "AnyInteger (I16|I24|I32)"),
            TypeCategory::AnyFloat => write!(f, "AnyFloat (F32|F64)"),
            TypeCategory::Specific(t) => write!(f, "{}", t),
            TypeCategory::Any => write!(f, "Any"),
        }
    }
}

/// Contrainte de type pour un node
#[derive(Debug, Clone)]
pub struct TypeRequirement {
    /// Catégorie de type acceptée
    pub category: TypeCategory,
    /// Types spécifiques acceptés (pour contraintes plus fines)
    /// Si vide, utilise category.possible_types()
    pub accepted_types: Vec<SampleType>,
}

impl TypeRequirement {
    /// Crée une contrainte pour n'importe quel type
    pub fn any() -> Self {
        Self {
            category: TypeCategory::Any,
            accepted_types: vec![],
        }
    }

    /// Crée une contrainte pour n'importe quel entier
    pub fn any_integer() -> Self {
        Self {
            category: TypeCategory::AnyInteger,
            accepted_types: vec![],
        }
    }

    /// Crée une contrainte pour n'importe quel flottant
    pub fn any_float() -> Self {
        Self {
            category: TypeCategory::AnyFloat,
            accepted_types: vec![],
        }
    }

    /// Crée une contrainte pour un type spécifique
    pub fn specific(sample_type: SampleType) -> Self {
        Self {
            category: TypeCategory::Specific(sample_type),
            accepted_types: vec![sample_type],
        }
    }

    /// Crée une contrainte avec une liste explicite de types acceptés
    pub fn from_list(types: Vec<SampleType>) -> Self {
        // Déterminer la catégorie la plus appropriée
        let all_integer = types.iter().all(|t| t.is_integer());
        let all_float = types.iter().all(|t| t.is_float());

        let category = if types.len() == 1 {
            TypeCategory::Specific(types[0])
        } else if all_integer && types.len() == 3 {
            TypeCategory::AnyInteger
        } else if all_float && types.len() == 2 {
            TypeCategory::AnyFloat
        } else if types.len() == 5 {
            TypeCategory::Any
        } else {
            // Catégorie personnalisée - on garde la liste explicite
            TypeCategory::Any
        };

        Self {
            category,
            accepted_types: types,
        }
    }

    /// Vérifie si cette contrainte accepte le type donné
    pub fn accepts(&self, sample_type: SampleType) -> bool {
        if !self.accepted_types.is_empty() {
            // Si une liste explicite est fournie, utiliser celle-ci
            self.accepted_types.contains(&sample_type)
        } else {
            // Sinon, utiliser la catégorie
            self.category.accepts(sample_type)
        }
    }

    /// Retourne tous les types acceptés par cette contrainte
    pub fn get_accepted_types(&self) -> Vec<SampleType> {
        if !self.accepted_types.is_empty() {
            self.accepted_types.clone()
        } else {
            self.category.possible_types()
        }
    }

    /// Vérifie si cette contrainte est plus restrictive qu'une autre
    pub fn is_more_restrictive_than(&self, other: &TypeRequirement) -> bool {
        let my_types = self.get_accepted_types();
        let other_types = other.get_accepted_types();

        // Je suis plus restrictif si tous mes types sont dans other_types
        // et que j'en ai moins
        my_types.iter().all(|t| other_types.contains(t)) && my_types.len() < other_types.len()
    }
}

impl fmt::Display for TypeRequirement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.accepted_types.is_empty() && self.accepted_types.len() < 5 {
            write!(
                f,
                "{}",
                self.accepted_types
                    .iter()
                    .map(|t| t.to_string())
                    .collect::<Vec<_>>()
                    .join("|")
            )
        } else {
            write!(f, "{}", self.category)
        }
    }
}

/// Vérifie la compatibilité entre un producer et un consumer
///
/// # Règles de compatibilité :
///
/// 1. Si le producer produit un type spécifique et que le consumer l'accepte → compatible
/// 2. Si le producer peut produire plusieurs types (ex: AnyInteger) et que le consumer
///    accepte un type spécifique (ex: I24), le producer POURRAIT produire un type
///    incompatible → **incompatible** (nécessite conversion explicite)
/// 3. Si le producer produit un type spécifique et que le consumer accepte une catégorie
///    contenant ce type → compatible
///
/// # Exemples :
///
/// - Producer(Specific(I24)) + Consumer(AnyInteger) → Compatible ✓
/// - Producer(AnyInteger) + Consumer(Specific(I24)) → Incompatible ✗ (producer peut produire I16)
/// - Producer(Specific(I24)) + Consumer(Specific(I24)) → Compatible ✓
/// - Producer(AnyInteger) + Consumer(AnyInteger) → Compatible ✓
pub fn check_compatibility(
    producer: &TypeRequirement,
    consumer: &TypeRequirement,
) -> Result<(), TypeMismatch> {
    let producer_types = producer.get_accepted_types();
    let consumer_types = consumer.get_accepted_types();

    // Vérifier si tous les types que le producer peut produire sont acceptés par le consumer
    for prod_type in &producer_types {
        if !consumer_types.contains(prod_type) {
            return Err(TypeMismatch {
                producer: producer.clone(),
                consumer: consumer.clone(),
                incompatible_type: Some(*prod_type),
            });
        }
    }

    Ok(())
}

/// Erreur de compatibilité de types
#[derive(Debug, Clone)]
pub struct TypeMismatch {
    /// Type requirement du producer
    pub producer: TypeRequirement,
    /// Type requirement du consumer
    pub consumer: TypeRequirement,
    /// Type spécifique incompatible (si identifié)
    pub incompatible_type: Option<SampleType>,
}

impl fmt::Display for TypeMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Type mismatch: producer produces {} but consumer only accepts {}",
            self.producer, self.consumer
        )?;
        if let Some(incomp_type) = self.incompatible_type {
            write!(f, " (incompatible type: {})", incomp_type)?;
        }
        Ok(())
    }
}

impl std::error::Error for TypeMismatch {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sample_type_is_integer() {
        assert!(SampleType::I16.is_integer());
        assert!(SampleType::I24.is_integer());
        assert!(SampleType::I32.is_integer());
        assert!(!SampleType::F32.is_integer());
        assert!(!SampleType::F64.is_integer());
    }

    #[test]
    fn test_sample_type_is_float() {
        assert!(!SampleType::I16.is_float());
        assert!(SampleType::F32.is_float());
        assert!(SampleType::F64.is_float());
    }

    #[test]
    fn test_type_category_accepts() {
        let any_int = TypeCategory::AnyInteger;
        assert!(any_int.accepts(SampleType::I16));
        assert!(any_int.accepts(SampleType::I24));
        assert!(any_int.accepts(SampleType::I32));
        assert!(!any_int.accepts(SampleType::F32));

        let specific = TypeCategory::Specific(SampleType::I24);
        assert!(!specific.accepts(SampleType::I16));
        assert!(specific.accepts(SampleType::I24));
        assert!(!specific.accepts(SampleType::I32));
    }

    #[test]
    fn test_compatibility_specific_to_category() {
        // Producer(Specific(I24)) + Consumer(AnyInteger) → Compatible
        let producer = TypeRequirement::specific(SampleType::I24);
        let consumer = TypeRequirement::any_integer();
        assert!(check_compatibility(&producer, &consumer).is_ok());
    }

    #[test]
    fn test_compatibility_category_to_specific() {
        // Producer(AnyInteger) + Consumer(Specific(I24)) → Incompatible
        let producer = TypeRequirement::any_integer();
        let consumer = TypeRequirement::specific(SampleType::I24);
        assert!(check_compatibility(&producer, &consumer).is_err());
    }

    #[test]
    fn test_compatibility_same_specific() {
        // Producer(Specific(I24)) + Consumer(Specific(I24)) → Compatible
        let producer = TypeRequirement::specific(SampleType::I24);
        let consumer = TypeRequirement::specific(SampleType::I24);
        assert!(check_compatibility(&producer, &consumer).is_ok());
    }

    #[test]
    fn test_compatibility_same_category() {
        // Producer(AnyInteger) + Consumer(AnyInteger) → Compatible
        let producer = TypeRequirement::any_integer();
        let consumer = TypeRequirement::any_integer();
        assert!(check_compatibility(&producer, &consumer).is_ok());
    }

    #[test]
    fn test_compatibility_integer_to_float() {
        // Producer(AnyInteger) + Consumer(AnyFloat) → Incompatible
        let producer = TypeRequirement::any_integer();
        let consumer = TypeRequirement::any_float();
        assert!(check_compatibility(&producer, &consumer).is_err());
    }

    #[test]
    fn test_type_requirement_from_list() {
        let req = TypeRequirement::from_list(vec![SampleType::I24, SampleType::I32]);
        assert!(req.accepts(SampleType::I24));
        assert!(req.accepts(SampleType::I32));
        assert!(!req.accepts(SampleType::I16));
        assert!(!req.accepts(SampleType::F32));
    }
}
