use pmoupnp::define_variable;

// Liste des conteneurs modifiés (format "id,updateId,id,updateId,...")
define_variable! {
    pub static CONTAINERUPDATEIDS: String = "ContainerUpdateIDs" {
        evented: true,
        // valeur initiale vide
    }
}
