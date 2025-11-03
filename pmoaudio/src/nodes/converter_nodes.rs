//! Nodes de conversion de type pour AudioChunk
//!
//! Ces nodes permettent de convertir les chunks audio d'un type vers un autre
//! (I16, I24, I32, F32, F64). Toutes les conversions utilisent les fonctions
//! DSP optimisées SIMD du module `crate::conversions`.
//!
//! Le designer de pipeline doit insérer manuellement ces nodes pour gérer
//! les incompatibilités de type entre producers et consumers.

use crate::{
    nodes::{AudioError, TypedAudioNode},
    type_constraints::{SampleType, TypeRequirement},
    AudioPipelineNode, AudioSegment,
};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

// Macro pour générer les converter nodes avec la nouvelle architecture AudioPipelineNode
macro_rules! converter_node {
    ($node_name:ident, $convert_method:ident, $output_type:expr, $doc:expr) => {
        #[doc = $doc]
        pub struct $node_name {
            tx: mpsc::Sender<Arc<AudioSegment>>,
            rx: mpsc::Receiver<Arc<AudioSegment>>,
            child_txs: Vec<mpsc::Sender<Arc<AudioSegment>>>,
            children: Vec<Box<dyn AudioPipelineNode>>,
        }

        impl $node_name {
            /// Crée un nouveau node de conversion
            pub fn new() -> Self {
                Self::with_channel_size(16)
            }

            /// Crée un nouveau node avec une taille de buffer spécifique
            pub fn with_channel_size(channel_size: usize) -> Self {
                let (tx, rx) = mpsc::channel(channel_size);
                Self {
                    tx,
                    rx,
                    child_txs: Vec::new(),
                    children: Vec::new(),
                }
            }
        }

        #[async_trait::async_trait]
        impl AudioPipelineNode for $node_name {
            fn get_tx(&self) -> Option<mpsc::Sender<Arc<AudioSegment>>> {
                Some(self.tx.clone())
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
                // Spawner tous les enfants
                let mut child_handles = Vec::new();
                for child in self.children {
                    let child_token = stop_token.child_token();
                    let handle = tokio::spawn(async move { child.run(child_token).await });
                    child_handles.push(handle);
                }

                // Boucle de traitement
                loop {
                    let segment = tokio::select! {
                        result = self.rx.recv() => {
                            match result {
                                Some(seg) => seg,
                                None => break,
                            }
                        }
                        _ = stop_token.cancelled() => {
                            break;
                        }
                    };

                    // Convertir si c'est un chunk audio, sinon passer tel quel
                    let output_segment = if segment.is_audio_chunk() {
                        if let Some(chunk) = segment.as_chunk() {
                            let converted_chunk = chunk.$convert_method();
                            Arc::new(AudioSegment {
                                order: segment.order,
                                timestamp_sec: segment.timestamp_sec,
                                segment: crate::_AudioSegment::Chunk(Arc::new(converted_chunk)),
                            })
                        } else {
                            segment
                        }
                    } else {
                        segment
                    };

                    // Envoyer à tous les enfants
                    for tx in &self.child_txs {
                        if tx.send(output_segment.clone()).await.is_err() {
                            // Un enfant est mort, arrêter
                            break;
                        }
                    }
                }

                // Attendre que tous les enfants se terminent
                for handle in child_handles {
                    match handle.await {
                        Ok(Ok(())) => {}
                        Ok(Err(e)) => return Err(e),
                        Err(e) => {
                            return Err(AudioError::ProcessingError(format!(
                                "Child task panicked: {}",
                                e
                            )))
                        }
                    }
                }

                Ok(())
            }
        }

        impl TypedAudioNode for $node_name {
            fn input_type(&self) -> Option<TypeRequirement> {
                Some(TypeRequirement::any())
            }

            fn output_type(&self) -> Option<TypeRequirement> {
                Some(TypeRequirement::specific($output_type))
            }
        }

        impl Default for $node_name {
            fn default() -> Self {
                Self::new()
            }
        }
    };
}

// Générer les 5 converter nodes
converter_node!(
    ToI16Node,
    to_i16,
    SampleType::I16,
    "Node de conversion vers I16 (16-bit signed integer)"
);
converter_node!(
    ToI24Node,
    to_i24,
    SampleType::I24,
    "Node de conversion vers I24 (24-bit signed integer)"
);
converter_node!(
    ToI32Node,
    to_i32,
    SampleType::I32,
    "Node de conversion vers I32 (32-bit signed integer)"
);
converter_node!(
    ToF32Node,
    to_f32,
    SampleType::F32,
    "Node de conversion vers F32 (32-bit floating point)"
);
converter_node!(
    ToF64Node,
    to_f64,
    SampleType::F64,
    "Node de conversion vers F64 (64-bit floating point)"
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AudioChunk, AudioChunkData};

    #[tokio::test]
    async fn test_to_f32_node_type_requirements() {
        let node = ToF32Node::new();

        // Vérifier les types d'entrée/sortie
        assert_eq!(
            node.input_type().unwrap().get_accepted_types().len(),
            5,
            "Should accept all 5 types"
        );
        assert_eq!(
            node.output_type()
                .unwrap()
                .get_accepted_types()
                .first()
                .copied(),
            Some(SampleType::F32),
            "Should output F32 only"
        );
    }

    // Nœud de test simple qui collecte les segments
    struct TestCollectorNode {
        input_tx: mpsc::Sender<Arc<AudioSegment>>,
        input_rx: mpsc::Receiver<Arc<AudioSegment>>,
        output_tx: mpsc::Sender<Arc<AudioSegment>>,
    }

    impl TestCollectorNode {
        fn new(output_tx: mpsc::Sender<Arc<AudioSegment>>) -> Self {
            let (input_tx, input_rx) = mpsc::channel(16);
            Self {
                input_tx,
                input_rx,
                output_tx,
            }
        }
    }

    #[async_trait::async_trait]
    impl AudioPipelineNode for TestCollectorNode {
        fn get_tx(&self) -> Option<mpsc::Sender<Arc<AudioSegment>>> {
            Some(self.input_tx.clone())
        }

        fn register(&mut self, _child: Box<dyn AudioPipelineNode>) {
            panic!("TestCollectorNode is a sink");
        }

        async fn run(
            mut self: Box<Self>,
            _stop_token: CancellationToken,
        ) -> Result<(), AudioError> {
            while let Some(segment) = self.input_rx.recv().await {
                if self.output_tx.send(segment).await.is_err() {
                    break;
                }
            }
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_to_i16_node_converts_from_i32() {
        let mut node = ToI16Node::new();
        let (out_tx, mut out_rx) = mpsc::channel(16);
        let collector = TestCollectorNode::new(out_tx);
        node.register(Box::new(collector));

        // Récupérer le tx du node
        let tx = node.get_tx().unwrap();

        // Lancer le node dans une tâche
        let stop_token = CancellationToken::new();
        let handle = tokio::spawn(async move { Box::new(node).run(stop_token).await });

        // Créer et envoyer un chunk I32
        let stereo = vec![[1_000_000i32 << 16, -500_000i32 << 16]; 100];
        let chunk_data = AudioChunkData::new(stereo.clone(), 48_000, 0.0);
        let chunk = AudioChunk::I32(chunk_data);
        let segment = Arc::new(AudioSegment {
            order: 0,
            timestamp_sec: 0.0,
            segment: crate::_AudioSegment::Chunk(Arc::new(chunk)),
        });

        tx.send(segment).await.unwrap();
        drop(tx);

        // Recevoir le chunk converti
        let result = out_rx.recv().await.unwrap();
        assert!(result.is_audio_chunk());

        if let Some(converted) = result.as_chunk() {
            assert_eq!(converted.type_name(), "i16");
            assert_eq!(converted.len(), 100);

            // Vérifier la conversion (downsampling de I32 vers I16)
            if let AudioChunk::I16(data) = &**converted {
                for (orig, converted_frame) in stereo.iter().zip(data.frames().iter()) {
                    let expected_l = (orig[0] >> 16) as i16;
                    let expected_r = (orig[1] >> 16) as i16;
                    assert_eq!(converted_frame[0], expected_l);
                    assert_eq!(converted_frame[1], expected_r);
                }
            }
        }

        handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn test_syncmarkers_passthrough() {
        let mut node = ToI16Node::new();
        let (out_tx, mut out_rx) = mpsc::channel(16);
        let collector = TestCollectorNode::new(out_tx);
        node.register(Box::new(collector));

        let tx = node.get_tx().unwrap();
        let stop_token = CancellationToken::new();

        tokio::spawn(async move {
            Box::new(node).run(stop_token).await.unwrap();
        });

        // Envoyer un syncmarker
        let top_zero = AudioSegment::new_top_zero_sync();
        tx.send(top_zero.clone()).await.unwrap();
        drop(tx);

        // Recevoir le syncmarker
        let result = out_rx.recv().await.unwrap();
        assert!(!result.is_audio_chunk());
        assert!(result.as_sync_marker().is_some());
    }
}
