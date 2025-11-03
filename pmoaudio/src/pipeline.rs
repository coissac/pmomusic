//! Architecture de pipeline audio avec propagation automatique du run et gestion d'arrêt
//!
//! Ce module définit le trait `AudioPipelineNode` qui permet de construire des arbres
//! de traitement audio avec :
//! - Démarrage automatique de tous les enfants lors du run de la tête
//! - Arrêt coordonné sur EOF ou erreur
//! - Propagation bidirectionnelle sans boucle infinie
//!
//! # Architecture
//!
//! Les pipelines forment des **arbres** (pas de DAG) où :
//! - Les sources n'ont pas d'input (get_tx retourne None)
//! - Les sinks n'ont pas d'enfants (register panic)
//! - Les convertisseurs ont à la fois un input et des enfants
//!
//! # Mécanisme d'arrêt
//!
//! - **Descendant** : `stop_token.cancel()` propage l'arrêt vers les fils
//! - **Montant** : Le retour de `run()` informe le parent
//! - **Détection** : Un enfant mort → parent voit `send().is_err()` ou `await handle`
//!
//! # Exemple
//!
//! ```no_run
//! use pmoaudio::{FileSource, AudioPipelineNode};
//! use pmoaudio::nodes::FlacFileSink;
//! use tokio_util::sync::CancellationToken;
//!
//! # async fn example() -> Result<(), pmoaudio::nodes::AudioError> {
//! // Construire le pipeline
//! let mut source = FileSource::new("input.flac");
//! let sink = FlacFileSink::new("output.flac");
//!
//! source.register(Box::new(sink));
//!
//! // Lancer avec contrôle d'arrêt
//! let stop_token = CancellationToken::new();
//! Box::new(source).run(stop_token).await?;
//! # Ok(())
//! # }
//! ```

use crate::{nodes::AudioError, AudioSegment};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Trait pour les nœuds d'un pipeline audio
///
/// Permet la construction d'arbres de traitement avec:
/// - Démarrage automatique de tous les enfants
/// - Arrêt coordonné sur EOF ou erreur
/// - Propagation bidirectionnelle sans boucle
#[async_trait::async_trait]
pub trait AudioPipelineNode: Send + 'static {
    /// Retourne un clone du sender pour recevoir des segments
    ///
    /// # Retourne
    ///
    /// - `Some(tx)` pour les nœuds qui ont un input (sinks, convertisseurs)
    /// - `None` pour les sources qui génèrent des données
    ///
    /// Le sender retourné est un clone, permettant au parent de l'extraire
    /// avant de consommer le nœud dans `run()`.
    fn get_tx(&self) -> Option<mpsc::Sender<Arc<AudioSegment>>>;

    /// Enregistre un nœud enfant dans l'arbre
    ///
    /// # Arguments
    ///
    /// * `child` - Le nœud enfant à enregistrer
    ///
    /// # Comportement
    ///
    /// - Pour les sources et convertisseurs : enregistre l'enfant et clone son tx
    /// - Pour les sinks : panic (nœuds terminaux)
    ///
    /// Le parent extrait le tx via `child.get_tx()` avant de stocker le child.
    fn register(&mut self, child: Box<dyn AudioPipelineNode>);

    /// Lance le nœud et tous ses enfants
    ///
    /// # Arguments
    ///
    /// * `stop_token` - Token d'arrêt partagé pour coordination
    ///
    /// # Comportement
    ///
    /// 1. **Spawn enfants** : Tous les enfants sont spawned avant le traitement
    /// 2. **Traitement** : Le nœud fait son travail (lecture, conversion, écriture)
    /// 3. **Détection** : Si un enfant meurt, le parent le détecte via send().is_err()
    /// 4. **Arrêt** : Sur EOF/erreur, appel de `stop_token.cancel()` pour les enfants
    /// 5. **Attente** : Attend que tous les enfants se terminent
    /// 6. **Retour** : Retourne pour informer le parent (propagation montante)
    ///
    /// # Propagation d'erreur
    ///
    /// - Erreur du nœud → propagée vers les enfants (cancel) puis vers le parent (return)
    /// - Erreur d'un enfant → détectée à l'await du handle, propagée vers le parent
    ///
    /// # Arrêt sans boucle
    ///
    /// - Un seul `cancel()` par nœud (en sortant de la boucle de travail)
    /// - L'enfant ne cancel JAMAIS le parent
    /// - `cancel()` est idempotent (pas de problème si appelé plusieurs fois)
    async fn run(
        self: Box<Self>,
        stop_token: CancellationToken,
    ) -> Result<(), AudioError>;
}
