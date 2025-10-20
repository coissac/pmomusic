//! Structures de l'enveloppe SOAP

use xmltree::Element;

/// Enveloppe SOAP complète
#[derive(Debug, Clone)]
pub struct SoapEnvelope {
    /// En-tête SOAP optionnel
    pub header: Option<SoapHeader>,

    /// Corps SOAP contenant l'action ou la réponse
    pub body: SoapBody,
}

/// En-tête SOAP
#[derive(Debug, Clone)]
pub struct SoapHeader {
    /// Contenu XML brut de l'en-tête
    pub content: Element,
}

/// Corps SOAP
#[derive(Debug, Clone)]
pub struct SoapBody {
    /// Contenu XML brut du corps
    pub content: Element,
}

impl SoapEnvelope {
    /// Crée une nouvelle enveloppe SOAP
    pub fn new(body: SoapBody) -> Self {
        Self { header: None, body }
    }

    /// Crée une nouvelle enveloppe avec header
    pub fn with_header(header: SoapHeader, body: SoapBody) -> Self {
        Self {
            header: Some(header),
            body,
        }
    }
}
