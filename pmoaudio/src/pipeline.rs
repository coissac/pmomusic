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
use tokio::task::JoinHandle;
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

    /// Lance le pipeline en arrière-plan et retourne un handle de contrôle
    ///
    /// Cette méthode est recommandée pour la plupart des cas d'usage.
    /// Elle spawn le pipeline dans une tâche Tokio et retourne immédiatement
    /// un `PipelineHandle` permettant de contrôler et surveiller l'exécution.
    ///
    /// # Retour
    ///
    /// Un `PipelineHandle` qui permet de :
    /// - Arrêter le pipeline avec `stop()`
    /// - Attendre sa complétion avec `wait()`
    /// - Vérifier son état avec `is_finished()`
    ///
    /// # Exemple
    ///
    /// ```no_run
    /// use pmoaudio::{FileSource, AudioPipelineNode};
    /// use pmoaudio::nodes::FlacFileSink;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut source = FileSource::new("input.flac");
    /// source.register(Box::new(FlacFileSink::new("output.flac")));
    ///
    /// // Lancer le pipeline
    /// let handle = Box::new(source).start();
    ///
    /// // Faire autre chose...
    /// tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    ///
    /// // Arrêter et attendre
    /// handle.stop(None);
    /// handle.wait().await?;
    /// # Ok(())
    /// # }
    /// ```
    fn start(self: Box<Self>) -> PipelineHandle {
        let stop_token = CancellationToken::new();
        let token_for_task = stop_token.clone();

        let join_handle = tokio::spawn(async move {
            self.run(token_for_task).await
        });

        PipelineHandle {
            stop_token,
            join_handle,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// NOUVELLE ARCHITECTURE - Séparation plomberie/logique métier
// ═══════════════════════════════════════════════════════════════════════════════

/// Raison de l'arrêt d'un nœud
///
/// Passé à la méthode `cleanup()` pour permettre au nœud d'adapter
/// son comportement de nettoyage selon la cause de l'arrêt.
#[derive(Debug, Clone)]
pub enum StopReason {
    /// Fin normale - toutes les données ont été traitées (EOF)
    Completed,

    /// Cancel explicite demandé via CancellationToken
    Cancelled,

    /// Un nœud enfant s'est terminé prématurément
    /// (dans un pipeline descendant, ceci est anormal)
    ChildFinished,

    /// Une erreur s'est produite (dans ce nœud ou un enfant)
    Error(AudioError),
}

/// Trait définissant la logique métier pure d'un nœud
///
/// Ce trait sépare la logique de traitement spécifique au nœud (ce qu'il **fait**)
/// de la plomberie d'orchestration (spawning, monitoring, cleanup).
///
/// # Responsabilités
///
/// - Recevoir des données via `input` (None pour les sources)
/// - Traiter les données selon la logique du nœud
/// - Envoyer les résultats via `output`
/// - Surveiller `stop_token` pour arrêt rapide
/// - Optionnellement : cleanup contextualisé via `cleanup()`
///
/// # Exemple
///
/// ```no_run
/// use pmoaudio::pipeline::NodeLogic;
/// use pmoaudio::nodes::AudioError;
/// use tokio_util::sync::CancellationToken;
///
/// struct MyProcessorLogic {
///     // Configuration du nœud
/// }
///
/// #[async_trait::async_trait]
/// impl NodeLogic for MyProcessorLogic {
///     async fn process(
///         &mut self,
///         input: Option<mpsc::Receiver<Arc<AudioSegment>>>,
///         output: Vec<mpsc::Sender<Arc<AudioSegment>>>,
///         stop_token: CancellationToken,
///     ) -> Result<(), AudioError> {
///         let mut rx = input.expect("Processor needs input");
///
///         loop {
///             tokio::select! {
///                 _ = stop_token.cancelled() => break,
///
///                 segment = rx.recv() => {
///                     match segment {
///                         Some(data) => {
///                             // Traiter les données
///                             let processed = self.do_processing(data)?;
///
///                             // Envoyer aux enfants
///                             for tx in &output {
///                                 tx.send(processed.clone()).await
///                                     .map_err(|_| AudioError::ChildDied)?;
///                             }
///                         }
///                         None => break, // EOF
///                     }
///                 }
///             }
///         }
///
///         Ok(())
///     }
/// }
/// ```
#[async_trait::async_trait]
pub trait NodeLogic: Send + 'static {
    /// Logique de traitement du nœud
    ///
    /// # Arguments
    ///
    /// * `input` - Receiver pour les données entrantes (None pour les sources)
    /// * `output` - Liste des senders vers les nœuds enfants
    /// * `stop_token` - Token pour détecter les demandes d'arrêt
    ///
    /// # Retour
    ///
    /// - `Ok(())` : Arrêt propre (EOF, cancelled)
    /// - `Err(...)` : Erreur de traitement
    ///
    /// # Comportement attendu
    ///
    /// - Surveiller `stop_token.cancelled()` dans la boucle principale
    /// - Sortir proprement sur EOF (input.recv() → None)
    /// - Gérer les erreurs de send (enfant mort) selon la politique du nœud
    async fn process(
        &mut self,
        input: Option<mpsc::Receiver<Arc<AudioSegment>>>,
        output: Vec<mpsc::Sender<Arc<AudioSegment>>>,
        stop_token: CancellationToken,
    ) -> Result<(), AudioError>;

    /// Cleanup appelé automatiquement après l'arrêt du nœud
    ///
    /// Cette méthode permet au nœud de faire du nettoyage contextualisé
    /// selon la raison de l'arrêt (fichiers incomplets, ressources, etc.)
    ///
    /// # Arguments
    ///
    /// * `reason` - La raison de l'arrêt du nœud
    ///
    /// # Implémentation par défaut
    ///
    /// Ne fait rien. Seulement les nœuds qui nécessitent un cleanup
    /// (ex: Sinks) doivent implémenter cette méthode.
    ///
    /// # Exemple
    ///
    /// ```no_run
    /// async fn cleanup(&mut self, reason: StopReason) -> Result<(), AudioError> {
    ///     match reason {
    ///         StopReason::Completed => {
    ///             // Finaliser le fichier proprement
    ///             self.flush_and_close().await?;
    ///         }
    ///         StopReason::Error(_) => {
    ///             // Supprimer le fichier incomplet
    ///             self.delete_incomplete_file().await?;
    ///         }
    ///         _ => {
    ///             // Autre cas selon politique
    ///         }
    ///     }
    ///     Ok(())
    /// }
    /// ```
    async fn cleanup(&mut self, _reason: StopReason) -> Result<(), AudioError> {
        Ok(())
    }
}

/// Handle pour contrôler un pipeline en cours d'exécution
///
/// Retourné par la méthode `start()`, ce handle permet de :
/// - Arrêter le pipeline explicitement
/// - Attendre sa complétion
/// - Vérifier s'il est toujours en cours
///
/// # Exemple
///
/// ```no_run
/// use pmoaudio::{FileSource, AudioPipelineNode};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let mut source = FileSource::new("input.flac");
/// // ... register children ...
///
/// let handle = Box::new(source).start();
///
/// // Faire autre chose...
/// tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
///
/// // Arrêter le pipeline
/// handle.stop(None);
///
/// // Attendre la fin
/// handle.wait().await?;
/// # Ok(())
/// # }
/// ```
pub struct PipelineHandle {
    stop_token: CancellationToken,
    join_handle: JoinHandle<Result<(), AudioError>>,
}

impl PipelineHandle {
    /// Demande l'arrêt du pipeline
    ///
    /// Cette méthode est non-bloquante. Pour attendre la fin effective,
    /// utiliser `wait()` ou `stop_and_wait()`.
    ///
    /// # Arguments
    ///
    /// * `reason` - Raison optionnelle de l'arrêt (pour logging/debugging)
    pub fn stop(&self, reason: Option<AudioError>) {
        if let Some(err) = reason {
            tracing::info!("Pipeline stop requested with error: {}", err);
        } else {
            tracing::info!("Pipeline stop requested");
        }
        self.stop_token.cancel();
    }

    /// Attendre la complétion du pipeline
    ///
    /// Bloque jusqu'à ce que le pipeline se termine (normalement ou par erreur).
    ///
    /// # Retour
    ///
    /// Le résultat du nœud racine :
    /// - `Ok(())` : Pipeline terminé avec succès
    /// - `Err(...)` : Erreur survenue dans le pipeline
    pub async fn wait(self) -> Result<(), AudioError> {
        match self.join_handle.await {
            Ok(result) => result,
            Err(e) if e.is_panic() => Err(AudioError::ProcessingError(
                format!("Pipeline task panicked: {}", e)
            )),
            Err(e) => Err(AudioError::ProcessingError(
                format!("Pipeline task cancelled: {}", e)
            )),
        }
    }

    /// Vérifie si le pipeline est toujours en cours d'exécution
    pub fn is_finished(&self) -> bool {
        self.join_handle.is_finished()
    }

    /// Arrête le pipeline et attend sa complétion
    ///
    /// Équivalent à `stop()` suivi de `wait()`.
    pub async fn stop_and_wait(self, reason: Option<AudioError>) -> Result<(), AudioError> {
        self.stop(reason);
        self.wait().await
    }

    /// Obtient une copie du token d'arrêt
    ///
    /// Pour cas d'usage avancés nécessitant une intégration
    /// avec d'autres systèmes utilisant CancellationToken.
    pub fn cancellation_token(&self) -> CancellationToken {
        self.stop_token.clone()
    }
}

/// Wrapper générique qui implémente l'orchestration d'un nœud
///
/// Cette struct encapsule n'importe quelle logique métier (implémentant `NodeLogic`)
/// et fournit l'implémentation standard du trait `AudioPipelineNode` avec :
/// - Spawning automatique des enfants
/// - Monitoring des enfants pour détection d'arrêt prématuré
/// - Cleanup coordonné avec propagation de cancel
/// - Appel automatique de `cleanup()` selon le contexte
///
/// # Type Parameters
///
/// * `L` - Le type implémentant `NodeLogic`, qui contient la logique spécifique du nœud
///
/// # Exemple
///
/// ```no_run
/// use pmoaudio::pipeline::{Node, NodeLogic};
///
/// struct MyLogic { /* ... */ }
/// impl NodeLogic for MyLogic { /* ... */ }
///
/// // Créer un nœud avec cette logique
/// let node = Node::new(MyLogic { /* ... */ });
/// ```
pub struct Node<L: NodeLogic> {
    /// La logique métier du nœud
    logic: L,

    /// Receiver pour les données entrantes (None pour les sources)
    rx: Option<mpsc::Receiver<Arc<AudioSegment>>>,

    /// Sender pour les données entrantes (pour clonage via get_tx)
    tx: Option<mpsc::Sender<Arc<AudioSegment>>>,

    /// Liste des nœuds enfants
    children: Vec<Box<dyn AudioPipelineNode>>,

    /// Liste des senders vers les enfants
    child_txs: Vec<mpsc::Sender<Arc<AudioSegment>>>,
}

impl<L: NodeLogic> Node<L> {
    /// Crée un nouveau nœud source (sans input)
    ///
    /// # Arguments
    ///
    /// * `logic` - La logique métier du nœud
    pub fn new_source(logic: L) -> Self {
        Self {
            logic,
            rx: None,
            tx: None,
            children: Vec::new(),
            child_txs: Vec::new(),
        }
    }

    /// Crée un nouveau nœud avec input (converter ou sink)
    ///
    /// # Arguments
    ///
    /// * `logic` - La logique métier du nœud
    /// * `buffer_size` - Taille du buffer du channel d'input
    pub fn new_with_input(logic: L, buffer_size: usize) -> Self {
        let (tx, rx) = mpsc::channel(buffer_size);
        Self {
            logic,
            rx: Some(rx),
            tx: Some(tx),
            children: Vec::new(),
            child_txs: Vec::new(),
        }
    }

    /// Retourne une référence vers la logique métier du nœud
    pub fn logic(&self) -> &L {
        &self.logic
    }

    /// Retourne une référence mutable vers la logique métier du nœud
    ///
    /// Permet de configurer la logique après construction mais avant run().
    /// Utile pour définir des options qui ne peuvent pas être connues
    /// au moment de la construction du nœud.
    pub fn logic_mut(&mut self) -> &mut L {
        &mut self.logic
    }
}

#[async_trait::async_trait]
impl<L: NodeLogic> AudioPipelineNode for Node<L> {
    fn get_tx(&self) -> Option<mpsc::Sender<Arc<AudioSegment>>> {
        self.tx.clone()
    }

    fn register(&mut self, child: Box<dyn AudioPipelineNode>) {
        if let Some(tx) = child.get_tx() {
            self.child_txs.push(tx);
        }
        self.children.push(child);
    }

    async fn run(
        mut self: Box<Self>,
        stop_token: CancellationToken,
    ) -> Result<(), AudioError> {
        let Node {
            mut logic,
            rx,
            children,
            child_txs,
            ..
        } = *self;

        tracing::info!("Node::run() starting with {} children", children.len());

        // ═══════════════════════════════════════════════════════════════════
        // PHASE 1: SPAWNER TOUS LES ENFANTS
        // ═══════════════════════════════════════════════════════════════════

        let mut child_handles = Vec::new();
        for (i, child) in children.into_iter().enumerate() {
            tracing::info!("Spawning child {}", i);
            let child_token = stop_token.child_token();
            let handle = tokio::spawn(async move {
                child.run(child_token).await
            });
            child_handles.push(handle);
        }
        tracing::info!("All {} children spawned", child_handles.len());

        // ═══════════════════════════════════════════════════════════════════
        // PHASE 2: MONITORER LES ENFANTS EN PARALLÈLE
        // ═══════════════════════════════════════════════════════════════════

        // Task qui surveille tous les enfants
        // Si pas d'enfants, retourne None pour indiquer qu'il n'y a rien à surveiller
        let mut child_monitor = if child_handles.is_empty() {
            tracing::debug!("No children to monitor (terminal node)");
            None
        } else {
            let handles = child_handles;
            let num_handles = handles.len();
            tracing::debug!("Child monitor starting with {} handles", num_handles);
            Some(tokio::spawn(async move {
                let mut has_error = false;
                let mut first_error = None;

                for handle in handles {
                    match handle.await {
                        Ok(Ok(())) => {
                            // Un enfant s'est terminé proprement
                            // C'est normal dans un pipeline linéaire
                            tracing::debug!("Child finished successfully");
                            continue;
                        }
                        Ok(Err(e)) => {
                            // Un enfant a eu une erreur
                            tracing::warn!("Child error: {}", e);
                            if !has_error {
                                first_error = Some(e);
                                has_error = true;
                            }
                            // Continue à surveiller les autres enfants
                        }
                        Err(e) => {
                            // Un enfant a paniqué
                            tracing::error!("Child panicked: {}", e);
                            if !has_error {
                                first_error = Some(AudioError::ProcessingError(
                                    format!("Child task panicked: {}", e)
                                ));
                                has_error = true;
                            }
                        }
                    }
                }

                // Retourner le résultat
                if let Some(err) = first_error {
                    Err(err)
                } else {
                    Ok(())
                }
            }))
        };

        // ═══════════════════════════════════════════════════════════════════
        // PHASE 3: EXÉCUTER LA LOGIQUE MÉTIER EN RACE AVEC LE MONITORING
        // ═══════════════════════════════════════════════════════════════════

        let (stop_reason, process_result, child_monitor_consumed) = if let Some(monitor) = &mut child_monitor {
            // Il y a des enfants à surveiller
            tokio::select! {
                // Cancel externe demandé
                _ = stop_token.cancelled() => {
                    tracing::debug!("Node cancelled via stop_token");
                    (StopReason::Cancelled, Ok(()), false)
                }

                // Monitoring des enfants - retourne quand tous sont terminés ou sur erreur
                child_result = monitor => {
                    match child_result {
                        Ok(Ok(())) => {
                            // Tous les enfants terminés avec succès
                            // Le parent devrait aussi terminer bientôt
                            tracing::debug!("All children finished successfully");
                            (StopReason::Completed, Ok(()), true)
                        }
                        Ok(Err(e)) => {
                            // Un enfant a eu une erreur - arrêter immédiatement
                            tracing::warn!("Child error: {}", e);
                            (StopReason::Error(e.clone()), Err(e), true)
                        }
                        Err(e) => {
                            // Le monitor task a paniqué
                            let error = AudioError::ProcessingError(
                                format!("Child monitor panicked: {}", e)
                            );
                            (StopReason::Error(error.clone()), Err(error), true)
                        }
                    }
                }

                // Logique métier du nœud
                process_result = logic.process(rx, child_txs.clone(), stop_token.clone()) => {
                    tracing::info!("Node logic.process() returned");
                    match process_result {
                        Ok(()) => {
                            tracing::info!("Node process completed successfully");
                            (StopReason::Completed, Ok(()), false)
                        }
                        Err(e) => {
                            tracing::error!("Node process error: {}", e);
                            (StopReason::Error(e.clone()), Err(e), false)
                        }
                    }
                }
            }
        } else {
            // Pas d'enfants (nœud terminal) - juste exécuter la logique
            tokio::select! {
                // Cancel externe demandé
                _ = stop_token.cancelled() => {
                    tracing::debug!("Node cancelled via stop_token");
                    (StopReason::Cancelled, Ok(()), true) // true car pas de monitor à attendre
                }

                // Logique métier du nœud
                process_result = logic.process(rx, child_txs.clone(), stop_token.clone()) => {
                    match process_result {
                        Ok(()) => {
                            tracing::debug!("Node process completed successfully (terminal)");
                            (StopReason::Completed, Ok(()), true) // true car pas de monitor
                        }
                        Err(e) => {
                            tracing::error!("Node process error: {}", e);
                            (StopReason::Error(e.clone()), Err(e), true) // true car pas de monitor
                        }
                    }
                }
            }
        };

        // ═══════════════════════════════════════════════════════════════════
        // PHASE 4: CLEANUP COORDONNÉ
        // ═══════════════════════════════════════════════════════════════════

        // 4.1 Fermer les channels pour signaler EOF aux enfants
        // Ceci permet aux enfants de finir de traiter les données restantes
        drop(child_txs);

        // 4.2 Cancel pour arrêt d'urgence seulement en cas d'erreur ou d'annulation
        // Si le nœud s'est terminé normalement, on laisse les enfants finir tranquillement
        match &stop_reason {
            StopReason::Completed => {
                // Fin normale - les enfants vont se terminer naturellement après avoir traité les données
                tracing::debug!("Node completed, letting children finish naturally");
            }
            StopReason::Cancelled | StopReason::ChildFinished | StopReason::Error(_) => {
                // Erreur ou annulation - forcer l'arrêt des enfants
                tracing::debug!("Cancelling children due to: {:?}", stop_reason);
                stop_token.cancel();
            }
        }

        // 4.3 Attendre que les enfants finissent (si child_monitor n'a pas été consommé dans le select!)
        if !child_monitor_consumed {
            if let Some(monitor) = child_monitor {
                tracing::debug!("Waiting for children to finish...");
                match monitor.await {
                    Ok(Ok(())) => {
                        tracing::debug!("All children finished successfully");
                    }
                    Ok(Err(e)) => {
                        tracing::warn!("Child error during cleanup: {}", e);
                        // Si on n'avait pas d'erreur avant, propager celle-ci
                        if process_result.is_ok() {
                            return Err(e);
                        }
                    }
                    Err(e) => {
                        tracing::error!("Child monitor panicked during cleanup: {}", e);
                    }
                }
            } else {
                tracing::debug!("No children to wait for (terminal node)");
            }
        }

        // 4.4 Cleanup du nœud (contextualisé selon la raison)
        if let Err(cleanup_err) = logic.cleanup(stop_reason).await {
            tracing::error!("Cleanup failed: {}", cleanup_err);
            // Si le cleanup échoue, propager cette erreur si process_result était Ok
            if process_result.is_ok() {
                return Err(cleanup_err);
            }
        }

        // ═══════════════════════════════════════════════════════════════════
        // PHASE 5: RETOURNER LE RÉSULTAT
        // ═══════════════════════════════════════════════════════════════════

        process_result
    }
}
