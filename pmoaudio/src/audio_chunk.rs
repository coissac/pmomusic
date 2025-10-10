use std::sync::Arc;

/// Représente un chunk audio stéréo avec données partagées via Arc
///
/// Cette structure encapsule des données audio stéréo (canaux gauche et droit)
/// en utilisant `Arc<Vec<f32>>` pour permettre le partage efficace entre plusieurs
/// consumers sans copier les données audio.
///
/// # Optimisation zero-copy
///
/// Les données audio sont wrappées dans `Arc`, ce qui signifie que:
/// - Le clonage d'un `AudioChunk` ne clone que les pointeurs Arc (très rapide)
/// - Les données audio réelles ne sont copiées que si nécessaire (Copy-on-Write)
/// - Plusieurs nodes peuvent partager le même chunk simultanément
///
/// # Exemples
///
/// ```
/// use pmoaudio::AudioChunk;
///
/// // Créer un chunk avec des données générées
/// let left = vec![0.0, 0.1, 0.2, 0.3];
/// let right = vec![0.0, 0.1, 0.2, 0.3];
/// let chunk = AudioChunk::new(0, left, right, 48000);
///
/// assert_eq!(chunk.len(), 4);
/// assert_eq!(chunk.sample_rate, 48000);
/// ```
#[derive(Debug, Clone)]
pub struct AudioChunk {
    /// Numéro d'ordre du chunk dans le flux
    ///
    /// Permet de suivre l'ordre des chunks et détecter les pertes éventuelles
    pub order: u64,

    /// Canal gauche (partagé via Arc pour éviter les clonages)
    ///
    /// Les samples sont en format float 32-bit, normalement entre -1.0 et 1.0
    pub left: Arc<Vec<f32>>,

    /// Canal droit (partagé via Arc pour éviter les clonages)
    ///
    /// Les samples sont en format float 32-bit, normalement entre -1.0 et 1.0
    pub right: Arc<Vec<f32>>,

    /// Taux d'échantillonnage en Hz
    ///
    /// Valeurs typiques: 44100, 48000, 96000, 192000
    pub sample_rate: u32,
}

impl AudioChunk {
    /// Crée un nouveau chunk audio
    ///
    /// Les vecteurs sont automatiquement wrappés dans `Arc`.
    ///
    /// # Arguments
    ///
    /// * `order` - Numéro d'ordre du chunk dans le flux
    /// * `left` - Samples du canal gauche
    /// * `right` - Samples du canal droit
    /// * `sample_rate` - Taux d'échantillonnage en Hz
    ///
    /// # Exemples
    ///
    /// ```
    /// use pmoaudio::AudioChunk;
    ///
    /// let chunk = AudioChunk::new(
    ///     0,
    ///     vec![0.0, 0.5, 1.0],
    ///     vec![0.0, 0.5, 1.0],
    ///     48000
    /// );
    /// ```
    pub fn new(order: u64, left: Vec<f32>, right: Vec<f32>, sample_rate: u32) -> Self {
        Self {
            order,
            left: Arc::new(left),
            right: Arc::new(right),
            sample_rate,
        }
    }

    /// Crée un chunk à partir de données déjà wrappées dans Arc
    ///
    /// Utile pour éviter un double wrapping si les données sont déjà dans Arc.
    pub fn from_arc(
        order: u64,
        left: Arc<Vec<f32>>,
        right: Arc<Vec<f32>>,
        sample_rate: u32,
    ) -> Self {
        Self {
            order,
            left,
            right,
            sample_rate,
        }
    }

    /// Retourne le nombre d'échantillons par canal
    ///
    /// # Exemples
    ///
    /// ```
    /// use pmoaudio::AudioChunk;
    ///
    /// let chunk = AudioChunk::new(0, vec![0.0; 1000], vec![0.0; 1000], 48000);
    /// assert_eq!(chunk.len(), 1000);
    /// ```
    pub fn len(&self) -> usize {
        self.left.len()
    }

    /// Vérifie si le chunk est vide
    pub fn is_empty(&self) -> bool {
        self.left.is_empty()
    }

    /// Clone les données pour permettre une modification (Copy-on-Write)
    ///
    /// Cette méthode doit être appelée uniquement si vous avez besoin de modifier
    /// les données audio. Pour une simple lecture, utilisez directement les champs
    /// `left` et `right`.
    ///
    /// # Exemples
    ///
    /// ```
    /// use pmoaudio::AudioChunk;
    ///
    /// let chunk = AudioChunk::new(0, vec![1.0, 2.0], vec![3.0, 4.0], 48000);
    /// let (mut left, mut right) = chunk.clone_data();
    ///
    /// // Modifier les données
    /// for sample in &mut left {
    ///     *sample *= 0.5;
    /// }
    /// ```
    pub fn clone_data(&self) -> (Vec<f32>, Vec<f32>) {
        ((*self.left).clone(), (*self.right).clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_chunk_creation() {
        let left = vec![0.0, 0.1, 0.2];
        let right = vec![0.0, 0.1, 0.2];
        let chunk = AudioChunk::new(0, left, right, 48000);

        assert_eq!(chunk.order, 0);
        assert_eq!(chunk.len(), 3);
        assert_eq!(chunk.sample_rate, 48000);
        assert!(!chunk.is_empty());
    }

    #[test]
    fn test_audio_chunk_arc_sharing() {
        let left = Arc::new(vec![0.0, 0.1, 0.2]);
        let right = Arc::new(vec![0.0, 0.1, 0.2]);

        let chunk1 = AudioChunk::from_arc(0, left.clone(), right.clone(), 48000);
        let chunk2 = chunk1.clone();

        // Vérifier que les Arc pointent vers les mêmes données
        assert!(Arc::ptr_eq(&chunk1.left, &chunk2.left));
        assert!(Arc::ptr_eq(&chunk1.right, &chunk2.right));
    }
}
