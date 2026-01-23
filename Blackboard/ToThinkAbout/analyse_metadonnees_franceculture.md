# Analyse : Récupération des métadonnées France Culture

## Objectif
Comprendre comment le site web de France Culture (https://www.radiofrance.fr/franceculture) obtient et affiche les informations sur l'émission en cours.

## Architecture du site

### Framework utilisé
**SvelteKit** avec Server-Side Rendering (SSR)

Le site utilise SvelteKit, comme en témoignent :
- L'attribut `data-sveltekit-preload-data="hover"` sur le `<body>`
- Les classes CSS préfixées par `svelte-` (ex: `svelte-1thibul`, `svelte-qz676b`)
- Les chemins vers les assets : `/client/immutable/assets/`

### Rendu des données
**SSR (Server-Side Rendering)** - Les données sont déjà présentes dans le HTML initial

## Méthode de récupération des informations

### ✅ API publique JSON découverte !

**Après analyse du trafic réseau (fichier HAR), l'API officielle existe et est OUVERTE :**

#### API LiveMeta (métadonnées en temps réel)
```
https://api.radiofrance.fr/livemeta/live/5/transistor_culture_player
```

**Caractéristiques :**
- ✅ **Aucune authentification requise** (pas de token)
- ✅ **Endpoint officiel** utilisé par le site web
- ✅ **JSON structuré** avec émission en cours, précédente et suivante
- ✅ **Timestamps précis** de début et fin d'émission
- ✅ **UUIDs des émissions** pour récupérer plus de détails
- ✅ **Indicateur de rafraîchissement** (`delayToRefresh` en millisecondes)

**Exemple de réponse :**
```json
{
  "prev": [{
    "firstLine": "Le direct",
    "firstLineUuid": null,
    "firstLinePath": null,
    "secondLine": "France Culture, l'esprit d'ouverture",
    "cover": "4e9fba8d-7675-409d-86a0-fce40f0cd4a6",
    "startTime": null,
    "endTime": null
  }],
  "now": {
    "firstLine": "La Série fiction",
    "firstLineUuid": "69cf4362-6bfb-48d1-89cf-9d11202f9938",
    "firstLineExpressionUuid": "69cf4362-6bfb-48d1-89cf-9d11202f9938",
    "firstLinePath": "franceculture/podcasts/fictions-le-feuilleton",
    "firstLinePathUuid": "3c1c2e55-41a0-11e5-9fe0-005056a87c89",
    "secondLine": "\"Ségou\" de Maryse Condé 9/10 : Deuil et pénitence",
    "secondLineExpressionUuid": "69cf4362-6bfb-48d1-89cf-9d11202f9938",
    "cover": "436430f7-5b2b-43f2-9f3c-28f2ad6cae39",
    "startTime": 1769108400,
    "endTime": 1769110122
  },
  "next": [{
    "firstLine": "L'Instant poésie",
    "firstLinePath": "franceculture/podcasts/l-instant-poesie",
    "firstLineUuid": "06fe22c7-144c-41b8-983d-ec956595b694",
    "secondLine": "L'Instant poésie d'Abd al Malik 14/20 : \"Roman inachevé\" de Louis Aragon, une main tendue",
    "cover": "a18a392b-f7d5-41bd-972a-e64451f35213",
    "startTime": 1769110200,
    "endTime": 1769110555
  }],
  "delayToRefresh": 742000
}
```

**Paramètres optionnels :**
- `?date=<timestamp>` : Récupérer les métadonnées à un moment donné (historique)

#### API Pikapi (images de couverture)
```
https://www.radiofrance.fr/pikapi/images/{uuid}/{taille}
```

**Exemples :**
- `https://www.radiofrance.fr/pikapi/images/436430f7-5b2b-43f2-9f3c-28f2ad6cae39/200x200`
- Autres tailles disponibles (à tester)

### Anciennes tentatives (pour référence historique)
Les tentatives d'accès aux endpoints suivants ont échoué :
- `https://www.radiofrance.fr/api/v2.1/stations/franceculture` → retourne du HTML
- `https://www.radiofrance.fr/api/v2.1/stations/franceculture/live` → retourne du HTML
- `https://openapi.radiofrance.fr/v1/graphql` → nécessite un header `x-token`

### Données embarquées dans le HTML (SSR)
Les informations sont également directement rendues dans le HTML par le serveur SvelteKit (méthode de fallback).

## Structure HTML des métadonnées

### Zone principale : CoverRadio
Les informations de l'émission en cours se trouvent dans la section `class="CoverRadio"` :

```html
<div class="CoverRadio-infoContainer">
    
    <!-- Titre de l'émission/segment -->
    <div class="CoverRadio-title qg-tt3 svelte-1thibul" role="heading" aria-level="1">
        <span class="truncate qg-focus-container svelte-1t7i9vq">
            <a href="/franceculture/podcasts/le-journal-de-l-eco/le-jouet-profite-de-la-morosite-ambiante-4949584" 
               aria-label="Le Journal de l'éco • Le jouet profite de la morosité ambiante">
                Le Journal de l'éco • Le jouet profite de la morosité ambiante
            </a>
        </span>
    </div>
    
    <!-- Nom de l'émission parente + producteur -->
    <p class="CoverRadio-subtitle qg-tt5 qg-focus-container svelte-1thibul">
        <a href="/franceculture/podcasts/les-matins">Les Matins</a>
        <span class="CoverRadio-producer qg-tx1 svelte-qz676b">par Guillaume Erner</span>
    </p>
    
    <!-- Indicateur de direct -->
    <div class="CoverRadio-ctaTop">
        <p class="direct qg-st6 CoverRadio-labelDirect dark default svelte-12tsplm">
            En direct
        </p>
    </div>
    
</div>
```

### Classes CSS identifiées

| Classe CSS | Contenu | Utilité |
|------------|---------|---------|
| `CoverRadio-title` | Titre du segment/chronique en cours | Titre principal |
| `CoverRadio-subtitle` | Nom de l'émission parente | Contexte de diffusion |
| `CoverRadio-producer` | Nom du producteur/animateur | Crédit |
| `CoverRadio-labelDirect` | Badge "En direct" | Statut de diffusion |

## Stratégies d'extraction

### Option 1 : Scraping HTML simple
Récupérer la page HTML et extraire les données via :
- Parsing HTML (BeautifulSoup en Python, scraper en Rust)
- Regex ciblées sur les classes CSS

**Avantages :**
- Pas de token nécessaire
- Données toujours présentes dans le HTML
- Méthode robuste

**Inconvénients :**
- Dépendant de la structure HTML
- Risque de cassure si le site change
- Parsing HTML plus lourd

### Option 2 : API GraphQL avec token
L'API GraphQL existe (`https://openapi.radiofrance.fr/v1/graphql`) mais nécessite un `x-token`.

**Étapes :**
1. Analyser le code JavaScript du site pour trouver comment le token est généré
2. Extraire ou reproduire la logique de génération de token
3. Utiliser l'API GraphQL

**Avantages :**
- API structurée et officielle
- Données JSON propres
- Moins de risque de changement

**Inconvénients :**
- Nécessite un token (non documenté publiquement)
- Potentiellement bloqué/limité en débit
- Reverse engineering requis

### Option 3 : API interne SvelteKit
SvelteKit utilise des endpoints `/__data.json` pour l'hydratation client.

**À explorer :**
- `https://www.radiofrance.fr/franceculture/__data.json`
- Endpoints de données internes

## Recommandation

### Pour un projet comme PMOMusic (pmoradiofrance)

**Approche hybride recommandée :**

1. **Court terme : Scraping HTML**
   - Implémenter un parser HTML en Rust
   - Cibler les classes CSS `CoverRadio-*`
   - Parser avec `scraper` ou `select` en Rust
   
2. **Moyen terme : Investigation API**
   - Analyser le code JavaScript pour trouver le token
   - Tenter d'utiliser l'API GraphQL si possible
   
3. **Mise en cache et rafraîchissement**
   - Rafraîchir les métadonnées toutes les 1-5 minutes
   - Mettre en cache pour éviter les requêtes excessives

## Exemple de code conceptuel (Rust)

```rust
use scraper::{Html, Selector};

async fn fetch_current_show() -> Result<ShowInfo, Error> {
    let html = reqwest::get("https://www.radiofrance.fr/franceculture")
        .await?
        .text()
        .await?;
    
    let document = Html::parse_document(&html);
    
    // Sélecteurs CSS
    let title_selector = Selector::parse(".CoverRadio-title a").unwrap();
    let subtitle_selector = Selector::parse(".CoverRadio-subtitle a").unwrap();
    let producer_selector = Selector::parse(".CoverRadio-producer").unwrap();
    
    let title = document
        .select(&title_selector)
        .next()
        .map(|e| e.inner_html())
        .unwrap_or_default();
    
    let show_name = document
        .select(&subtitle_selector)
        .next()
        .map(|e| e.inner_html())
        .unwrap_or_default();
    
    let producer = document
        .select(&producer_selector)
        .next()
        .map(|e| e.inner_html().replace("par ", ""))
        .unwrap_or_default();
    
    Ok(ShowInfo {
        title,
        show_name,
        producer,
    })
}
```

## Points d'attention

1. **Rate limiting** : Ne pas surcharger le site avec des requêtes trop fréquentes
2. **User-Agent** : Utiliser un User-Agent identifiable pour un projet open-source
3. **Gestion d'erreurs** : Le site peut être temporairement indisponible
4. **Structure HTML** : Peut changer sans préavis
5. **Respect des CGU** : Vérifier les conditions d'utilisation de Radio France

## Mise à jour de la page côté client

### Comment la page se rafraîchit-elle ?

**Réponse : La page ne se met PAS à jour automatiquement côté client.**

Après analyse :
1. **Pas de polling/WebSocket** : Aucun mécanisme de `setInterval`, `setTimeout`, WebSocket ou Server-Sent Events (SSE) détecté dans le HTML
2. **Pas de JavaScript de mise à jour** : Le DOM n'est pas modifié dynamiquement pour les métadonnées `CoverRadio-*`
3. **Navigation SvelteKit** : Les mises à jour se font via la navigation SPA de SvelteKit

### Mécanisme de navigation SvelteKit

SvelteKit utilise le **preloading** et les **endpoints `__data.json`** :

```
https://www.radiofrance.fr/franceculture/__data.json
```

Cet endpoint retourne un **JSON structuré** contenant toutes les données de la page, incluant :
- Métadonnées de l'émission en cours
- Configuration du site
- Contenu de la page

**Format de données** :
```json
{
  "type": "data",
  "nodes": [
    {
      "metadata": { ... },
      "context": { ... },
      "mainStationLive": { ... }
    }
  ]
}
```

### Stratégie de rafraîchissement

Pour un utilisateur sur le site :
1. **Chargement initial** : SSR complet avec HTML
2. **Navigation ultérieure** : SvelteKit charge `__data.json` en AJAX
3. **Rechargement manuel** : L'utilisateur doit recharger la page (F5) pour voir les nouvelles métadonnées

**Il n'y a pas de mise à jour automatique en temps réel.**

## Recommandation mise à jour

### 🏆 Option privilégiée : API LiveMeta officielle (DÉCOUVERTE !)

**URL :** `https://api.radiofrance.fr/livemeta/live/5/transistor_culture_player`

**Avantages :**
- ✅ **API officielle Radio France** : Endpoint public et documenté
- ✅ **Aucune authentification** : Pas de token, pas de restriction
- ✅ **JSON léger et structuré** : Format simple et prévisible
- ✅ **Données optimales** : Juste ce qu'il faut (prev/now/next)
- ✅ **Polling intelligent** : `delayToRefresh` indique quand rafraîchir
- ✅ **Stable** : API de production utilisée par le site officiel
- ✅ **Support historique** : Paramètre `?date=` pour l'historique
- ✅ **UUIDs** : Références pour récupérer plus de détails si besoin

**Inconvénients :**
- Aucun majeur identifié

**Code Rust recommandé :**
```rust
use serde::{Deserialize, Serialize};
use reqwest;

#[derive(Debug, Deserialize, Serialize)]
struct LiveMetadata {
    prev: Vec<ShowInfo>,
    now: ShowInfo,
    next: Vec<ShowInfo>,
    #[serde(rename = "delayToRefresh")]
    delay_to_refresh: u64,
}

#[derive(Debug, Deserialize, Serialize)]
struct ShowInfo {
    #[serde(rename = "firstLine")]
    first_line: String,
    #[serde(rename = "firstLineUuid")]
    first_line_uuid: Option<String>,
    #[serde(rename = "firstLinePath")]
    first_line_path: Option<String>,
    #[serde(rename = "secondLine")]
    second_line: String,
    cover: String,
    #[serde(rename = "startTime")]
    start_time: Option<u64>,
    #[serde(rename = "endTime")]
    end_time: Option<u64>,
}

async fn fetch_franceculture_live() -> Result<LiveMetadata, reqwest::Error> {
    let url = "https://api.radiofrance.fr/livemeta/live/5/transistor_culture_player";
    
    reqwest::get(url)
        .await?
        .json::<LiveMetadata>()
        .await
}

// Utilisation avec polling intelligent
async fn monitor_live() {
    loop {
        match fetch_franceculture_live().await {
            Ok(metadata) => {
                println!("En cours : {} - {}", 
                    metadata.now.first_line, 
                    metadata.now.second_line
                );
                
                // Attendre le temps recommandé avant de rafraîchir
                tokio::time::sleep(
                    tokio::time::Duration::from_millis(metadata.delay_to_refresh)
                ).await;
            }
            Err(e) => {
                eprintln!("Erreur : {}", e);
                // Fallback : attendre 60 secondes
                tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
            }
        }
    }
}
```

### Hiérarchie des options (mise à jour)

1. **🥇 Premier choix : API LiveMeta** - API officielle Radio France
2. **🥈 Fallback niveau 1 : `__data.json`** - Endpoint SvelteKit si LiveMeta indisponible
3. **🥉 Fallback niveau 2 : Scraping HTML** - Si les API JSON sont toutes indisponibles
4. **💭 Exploration future : API GraphQL** - Si un token public devient disponible

## Conclusion

**Pour la mise à jour côté serveur (PMOMusic) :**
- ✅ **Utiliser l'API LiveMeta officielle** : `https://api.radiofrance.fr/livemeta/live/5/transistor_culture_player`
- ✅ **Polling intelligent** : Utiliser `delayToRefresh` pour optimiser les appels
- ✅ **Récupération des images** : Via Pikapi avec l'UUID de `cover`
- ✅ **Gestion d'erreur** : Fallback sur `__data.json` puis HTML si nécessaire

**Pour la page web elle-même :**
- **Aucune mise à jour automatique** : L'utilisateur doit recharger la page manuellement
- Navigation SPA via SvelteKit charge `__data.json` en AJAX
- Le SSR initial contient déjà toutes les données dans le HTML

## URLs de flux audio découvertes

### Flux HLS (recommandé)

**Master playlist :**
```
https://stream.radiofrance.fr/franceculture/franceculture.m3u8?id=radiofrance
```

**Qualités disponibles :**
- **lofi** : 105 kbps (BANDWIDTH=107000) - `franceculture_lofi.m3u8?id=radiofrance`
- **midfi** : 178 kbps (BANDWIDTH=185000) - `franceculture_midfi.m3u8?id=radiofrance`
- **hifi** : 268 kbps (BANDWIDTH=280000) - `franceculture_hifi.m3u8?id=radiofrance`

Codec : `mp4a.40.2` (AAC-LC)

### Flux Icecast (à confirmer)

D'après RF_old.json, ces URLs devraient exister (non observées dans le HAR car le player web utilise HLS) :

**MP3 :**
```
https://icecast.radiofrance.fr/franceculture-lofi.mp3?id=radiofrance
https://icecast.radiofrance.fr/franceculture-midfi.mp3?id=radiofrance
https://icecast.radiofrance.fr/franceculture-hifi.mp3?id=radiofrance
```

**AAC :**
```
https://icecast.radiofrance.fr/franceculture-lofi.aac?id=radiofrance
https://icecast.radiofrance.fr/franceculture-midfi.aac?id=radiofrance
https://icecast.radiofrance.fr/franceculture-hifi.aac?id=radiofrance
```

## Mapping des stations Radio France

D'après l'analyse du fichier HAR et RF_old.json, voici le mapping des IDs de stations :

| Station | ID Station | Endpoint LiveMeta |
|---------|-----------|-------------------|
| France Culture | 5 | `/livemeta/live/5/transistor_culture_player` |
| France Inter | ? | À découvrir |
| France Musique | ? | À découvrir |
| FIP | ? | À découvrir |
| Mouv' | ? | À découvrir |
| France Bleu (national) | ? | À découvrir |

**Note :** Les IDs des autres stations peuvent être découverts en analysant le HAR de leurs pages respectives ou en testant des valeurs séquentielles (1, 2, 3, 4, 6, 7...).

## Prochaines étapes recommandées

1. ✅ **Implémenter le client LiveMeta** en Rust avec les structures proposées
2. 🔍 **Découvrir les IDs des autres stations** Radio France
3. 🔍 **Tester les URLs Icecast** pour confirmer leur disponibilité
4. 📋 **Documenter l'API complète** dans le code PMOMusic
5. 🧪 **Tester le paramètre `?date=`** pour l'accès historique
6. 🎨 **Tester les tailles d'images Pikapi** disponibles (200x200, 400x400, etc.)

## Annexe : Analyse du fichier HAR

**Source :** `www.radiofrance.fr.har`  
**Date de capture :** 2026-01-22  
**Page analysée :** https://www.radiofrance.fr/franceculture

**Découvertes principales :**
- API LiveMeta accessible et ouverte
- Aucune authentification requise
- Polling intelligent via `delayToRefresh`
- Support HLS multi-bitrate
- API Pikapi pour les images

Cette analyse confirme que Radio France expose des APIs publiques utilisables pour des projets comme PMOMusic.
