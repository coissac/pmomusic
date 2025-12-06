# Player Générique PMO Music

## Vue d'ensemble

Ce document décrit l'implémentation d'un nouveau player web générique qui utilise **uniquement** l'API du trait `pmosource` sans dépendre d'aucune implémentation spécifique (comme `pmoparadise`).

## Objectifs

L'objectif principal est de **tester l'API `pmosource` dans un cas d'application concret** afin d'identifier ce qui manque ou pourrait être amélioré dans l'API générique.

## Architecture

### 1. Service API TypeScript (`pmoapp/webapp/src/services/pmosource.ts`)

Service qui encapsule toutes les interactions avec l'API REST de pmosource :

```typescript
// Endpoints utilisés
GET /api/sources                          // Liste les sources
GET /api/sources/{id}                     // Info sur une source
GET /api/sources/{id}/root                // Container racine
GET /api/sources/{id}/browse              // Parcourt un container
GET /api/sources/{id}/resolve             // Résout l'URI d'un item
GET /api/sources/{id}/image               // Image de la source
GET /api/sources/{id}/capabilities        // Capacités de la source
```

**Fonctions implémentées :**
- `listSources()` - Liste toutes les sources enregistrées
- `getSource(id)` - Récupère une source spécifique
- `getSourceRoot(id)` - Récupère le container racine
- `browseSource(id, objectId?, pagination?)` - Navigation dans les containers
- `resolveUri(sourceId, objectId)` - Résout l'URI de streaming
- `getSourceImageUrl(id)` - URL de l'image de la source

### 2. Composant Player (`pmoapp/webapp/src/components/GenericMusicPlayer.vue`)

Composant Vue.js qui implémente :

#### Fonctionnalités implémentées

1. **Sélection de sources**
   - Affichage de toutes les sources disponibles
   - Affichage du logo de chaque source
   - Affichage des capacités (FIFO, Search, Favorites)

2. **Navigation dans les containers**
   - Breadcrumb pour remonter dans la hiérarchie
   - Affichage des sous-containers (dossiers)
   - Navigation par clic dans les containers

3. **Liste des morceaux**
   - Affichage de tous les items audio d'un container
   - Métadonnées : titre, artiste, album, cover art
   - Numérotation des morceaux

4. **Lecteur audio**
   - Lecture d'un morceau via résolution d'URI
   - Contrôles audio natifs HTML5
   - Section "Now Playing" avec métadonnées
   - Gestion des erreurs de lecture

5. **Interface utilisateur**
   - Design moderne avec dégradés et animations
   - Responsive design
   - Indicateurs visuels (morceau actif, en cours de lecture)
   - Messages d'erreur clairs

### 3. Intégration

Le player a été configuré comme **page d'accueil par défaut** de l'application web PMO :

```typescript
// router/index.ts
const routes = [
  { path: "/", name: "home", component: GenericMusicPlayer },
  // ... autres routes
]
```

## Ce qui fonctionne

✅ **Complètement fonctionnel avec l'API actuelle de pmosource :**

1. Découverte des sources disponibles
2. Navigation complète dans la hiérarchie des containers
3. Affichage des métadonnées des morceaux
4. Résolution des URIs et lecture audio
5. Affichage des images de sources
6. **Métadonnées temps réel via Server-Sent Events (SSE)** 🆕
   - Mise à jour automatique toutes les 3 secondes
   - Pas de polling, push serveur
   - Reconnexion automatique

## Limitations identifiées et améliorations possibles

### 1. Métadonnées de couverture d'album

**Problème :** Le trait `MusicSource` n'expose pas directement de méthode pour résoudre les URIs de couvertures d'album.

**État actuel :**
- Le champ `album_art` dans `Item` contient parfois une URI
- Le champ `album_art_pk` contient une clé primaire mais pas d'URL exploitable directement
- Certaines implémentations (pmoparadise) utilisent `/cache/cover/{pk}` mais ce n'est pas standardisé

**Proposition :**
```rust
/// Résout l'URI de la couverture d'album pour un item
async fn resolve_cover_uri(&self, object_id: &str) -> Result<Option<String>>;
```

### 2. Recherche globale

**Problème :** La méthode `search()` existe mais retourne `SearchNotSupported` par défaut.

**État actuel :**
- Pas d'interface standardisée pour la recherche dans l'UI
- Pas de retour clair sur les capacités de recherche

**Proposition :**
- Utiliser `capabilities().supports_search` pour afficher/masquer l'UI de recherche
- Documenter clairement le format attendu des requêtes de recherche

### 3. Pagination

**Problème :** L'API supporte la pagination mais les métadonnées ne permettent pas de connaître le nombre total d'items.

**État actuel :**
- `BrowseResponse.total` retourne le nombre d'items retournés, pas le total disponible
- Pas de méthode `get_total_count(object_id)` dans le trait

**Proposition :**
```rust
/// Retourne le nombre total d'items dans un container
async fn get_total_count(&self, object_id: &str) -> Result<usize>;
```

Ou ajouter `total_available` dans `BrowseResponse` :
```rust
pub struct SourceBrowseResponse {
    // ... champs existants
    pub total_available: Option<usize>, // Total disponible (pas juste retourné)
}
```

### 4. Métadonnées de stream en temps réel ✅ **IMPLÉMENTÉ**

**Solution implémentée :**
- ✅ Méthode `get_item(object_id)` dans le trait `MusicSource`
- ✅ Endpoint REST `GET /api/sources/{id}/item?object_id={id}` pour récupérer les métadonnées d'un item
- ✅ Endpoint SSE `GET /api/sources/{id}/item/stream?object_id={id}` pour recevoir les mises à jour en temps réel
- ✅ Le player web utilise Server-Sent Events (SSE) pour les métadonnées temps réel

**Comment ça fonctionne :**
1. Le serveur envoie automatiquement les métadonnées à jour toutes les 3 secondes via SSE
2. Le client se connecte avec `EventSource` (API browser native)
3. Les métadonnées sont automatiquement mises à jour dans l'interface sans polling

**Pour RadioParadise :**
- La méthode `get_item()` pour les live streams récupère les métadonnées depuis `/radioparadise/metadata/{slug}`
- Le SSE permet d'avoir les métadonnées à jour en moins de 3 secondes (au lieu de 10 secondes avec le polling)

### 5. Playlists utilisateur

**Problème :** Les méthodes existent (`get_user_playlists()`, `add_to_playlist()`) mais retournent `NotSupported` par défaut.

**État actuel :**
- Pas encore testé dans le player
- Nécessiterait une UI dédiée

**Proposition :**
- Créer une section "Playlists" dans le player
- Tester l'API avec une implémentation qui supporte les playlists (ex: Qobuz)

### 6. Favoris

**Problème :** Similaire aux playlists, l'API existe mais n'est pas testée.

**Proposition :**
- Ajouter un bouton "⭐ Favoris" sur chaque morceau
- Afficher visuellement les morceaux favoris
- Créer une section "Mes Favoris"

### 7. Auto-play / Queue

**Problème :** Il n'y a pas de méthode pour gérer une file d'attente de lecture.

**Proposition :**
```rust
/// Interface pour gérer une queue de lecture
pub trait Playable: MusicSource {
    async fn get_next_track(&self) -> Result<Option<Item>>;
    async fn get_previous_track(&self) -> Result<Option<Item>>;
    async fn add_to_queue(&self, item: Item) -> Result<()>;
    async fn clear_queue(&self) -> Result<()>;
    async fn get_queue(&self) -> Result<Vec<Item>>;
}
```

### 8. Durée totale d'un container

**Problème :** Pour afficher "Album: 45:32 min, 12 morceaux", il faut parcourir tous les items.

**Proposition :**
```rust
/// Statistiques d'un container spécifique
async fn get_container_stats(&self, object_id: &str) -> Result<ContainerStats>;

pub struct ContainerStats {
    pub item_count: usize,
    pub total_duration_ms: Option<u64>,
    pub total_size_bytes: Option<u64>,
}
```

### 9. Formats audio disponibles

**Problème :** La méthode `get_available_formats()` existe mais n'est pas exploitée dans l'UI.

**Proposition :**
- Ajouter un sélecteur de qualité dans le player
- Afficher les formats disponibles (FLAC 24/96, MP3 320, etc.)

### 10. État du cache

**Problème :** Les méthodes existent (`get_cache_status()`, `cache_item()`) mais ne sont pas intégrées.

**Proposition :**
- Afficher un indicateur de cache sur chaque morceau
- Bouton "📥 Télécharger" pour mettre en cache
- Barre de progression pour le téléchargement

## Prochaines étapes

### Court terme
1. ✅ Tester le player avec `pmoparadise` (déjà implémenté)
2. 🔄 Identifier les bugs et limitations pratiques
3. 🔄 Tester avec une deuxième source (ex: `pmoqobuz`) pour valider la généricité

### Moyen terme
1. Implémenter les fonctionnalités manquantes identifiées ci-dessus
2. Ajouter la gestion de queue et auto-play
3. Ajouter la recherche si supportée
4. Intégrer la gestion du cache

### Long terme
1. Support des playlists utilisateur
2. Support des favoris
3. Égaliseur et effets audio
4. Visualisations audio
5. Mode hors-ligne avec cache

## Conclusion

Le player générique démontre que **l'API `pmosource` est déjà très utilisable** pour créer une application musicale fonctionnelle. Les principales limitations concernent :

1. **Les métadonnées de couvertures** (pas d'URL standardisée)
2. **La pagination avancée** (pas de compte total)
3. **Les métadonnées temps réel** (pour les streams live)
4. **La gestion de queue** (pas d'API dédiée)

Ces limitations ne sont pas bloquantes mais leur résolution améliorerait significativement l'expérience utilisateur et la complétude de l'API.

## Utilisation

Pour tester le player :

1. Lancer le serveur backend avec au moins une source enregistrée :
   ```bash
   cargo run --example single_channel_server --features full
   ```

2. Accéder à l'application web :
   ```
   http://localhost:8080/app/
   ```

3. Le player devrait afficher automatiquement les sources disponibles et permettre la navigation et la lecture.

## Remarques importantes

- ✅ Le player **n'utilise QUE l'API pmosource générique**
- ✅ Aucune dépendance sur `pmoparadise` ou toute autre implémentation spécifique
- ✅ Tout est basé sur les endpoints REST de `pmosource::api`
- ✅ Le code est totalement réutilisable pour toute nouvelle source (Qobuz, Spotify, etc.)
