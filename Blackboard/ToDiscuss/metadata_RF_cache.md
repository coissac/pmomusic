**Tu réaliseras ce travail en appliquant scrupuleusement les règles définies dans [@Rules.md](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/Blackboard/Rules.md)**

**On ne travaille que dans la Crate PMORadioFrance**

À partir de maintenant, tu ne prends plus en compte ce que tu pensais avant et tu écoutes bien. Et tu construis un plan d'implémentation que je dois valider. Tu arrêtes de prendre des initiatives et de faire des bêtises.

- Tu as des fonctions d'interrogation de l'API Radio France qu'il faut utiliser au minimum. Mais Radio France nous donne des dates d'invalidation des métadonnées. Globalement, on doit gérer une grosse map où les valeurs ont des TTL. 
- Quand un client demande une donnée du cache, si le TTL est atteint, il commence par utiliser l'API Radio France, modifie le cache, puis la retourne. Dans le cas contraire, il retourne directement la donnée.

A chaque fois qu'il fait un appel de l'API Radio France pour modifier ses valeurs, Le cache émet un événement ou avertit ses abonnés, comme quoi les données d'un slug particulier ont été modifiées. Comme ça tout le monde peut se synchroniser.

Les clients, par exemple la fonction Browse, n'interrogent que le cache qui a forcément des données à jour. Les métadonnées ne sont jamais stockées hors du cache, on se réfère toujours à elles.

Nous devons maintenant considérer le fonctionnement du control point. Celui-ci est capable de s'abonner à une playlist pour suivre ses modifications. Il ne peut pas s'abonner à un item. 

Dans le cas d'une radio, on peut considérer que chaque canal, chaque slug, est en réalité une playlist à un item qu'il faut suivre. Ainsi, le Control Point peut décider de jouer cette playlist en s'abonnant à elle et être tenu au courant des modifications par des événements GENA.

La source Radio France doit donc s'abonner aux événements du Cache. A chaque fois qu'un slug est modifié, elle avertit par un événement Jenna que la playlist à un item qui correspond à ce Slug est modifiée.

Maintenant, il y a le cache des stations. Le cache des stations finalement il ne stock qu'un emboîtement de listes de slug. Ça, normalement, ça ne bouge quasiment pas. On peut dire que une fois par jour, on met à jour ce cache. Les listes de slug ont donc un TTL mais très long. 

A chaque browse, on reconstruit un document didl à partir des métadonnées à jour provenant du cache.

## Round 2

Je repasse sur ton code. Tout est beaucoup beaucoup trop compliqué, trop de structures allambiquées, de trucs qui s'emboîtent dans des trucs. Il faut faire simple. Le mot d'ordre est simple. Nous ne construisons pas une usine à gaz, nous construisons simplement un truc capable de diffuser moins d'une centaine de radios.

### Simplification de la notion de station.

Alors, tu fais une distinction entre radio locale et web radio, c'est une distinction sémantique, mais d'un point de vue informatique y'a pas de différence.

L'unité de base, ça devrait être:

pub struct StationGroup {
    pub stations: Vec<Station>,
}

La seule règle metier sémantique est: L'index 0 du vecteur est attribué à la station principale du groupe, par exemple FIP, pour le groupe FIP, si elle existe.

Et du coup, les StationGroups devrait juste être un vecteur de StationGroup

- StationGroups définie le niveau zéro du browse
- StationGroup définit les différents niveaux 1

Chaque station étant représentée maintenant par une playlist à un item item, Il y a un niveau 2 de browsing qui correspond à l'item de la station.

Donc, Station, StationGroup et StationGroups devrait chacun fournir une méthode retournant un objet PMODidl qui se construit en demandant les métadonnées au cache. Genre:

async pub fn to_didl(caches et server_base_url)

## Simplification du cache

Il faut réfléchir, Finalement, qu'est-ce que l'on a besoin de stocker dans le cache pour être efficace? De quoi remplir les Didl. Donc, à partir des données parsées depuis l'API Radio France, il faut reconstruire une structure simplifiée. contenant juste les données telles qu'on va les utiliser dans le diddle. Idéalement, le cache devrait être capable de fournir le bien d'idoles d'un item. Avec une méthode to_didl(slug) -> Un item de la Crate pmodidl. Tout le reste est superflu. Donc ne doit pas être stocké. Pour calculer la durée correctement, Il nous faut la fin de validité de l'item. Il est donc important de stocker end_time. Normalement, end time est aussi le TTL. Car à la fin de la diffusion de cet item, ça veut dire qu'il faut remettre à jour les métadata, Pour avoir l'item suivant.

## Round 3

### Problèmes identifiés

Point 3 : Le cache des slugs doit être persistant et stocké dans la config comme actuellement. Avec un délai d'une semaine. Le cache des métadonnées reste en mémoire. Les métadonnées changent à chaque émission, il n'y a pas de raison de les stocker de manière persistante.

Du coup, le cache des métadonnées, Pour simplifier la vie des autres structures. devrait s'occuper de cacher les covers dans pmocovers et stocker le PK de l'image dans le cache pour pouvoir construire le didl de l'item. 

En fait, le didl de l'item, Dans notre nouvelle strategie est déjà un didl d'une playlist à un item.

### Architecture cible simplifiée

#### 1. Structures de station (models.rs et playlist.rs)

Je ne comprends pas bien la distinction entre les deux:
A-t-on vraiment besoin des deux fonctions?
À quoi sert cette fonction to_container?

```
// Browse niveau 1: retourne les playlists (containers) pour chaque station du groupe
    pub async fn to_didl(&self, metadata_cache: &MetadataCache, server_base_url: &str) -> Vec<Container>;
    
    // Helper pour construire le container de groupe (sans items, juste la structure)
    pub fn to_container(&self, server_base_url: Option<&str>) -> Container;
```

```
impl CachedMetadata {
    // Parse depuis LiveResponse + Station + optionnel cover cache
    #[cfg(feature = "cache")]
    pub async fn from_live_response(
        station: &Station,
        live: &LiveResponse,
        cover_cache: Option<&Arc<CoverCache>>,
        server_base_url: Option<&str>,
    ) -> Result<Self>;
    
    pub fn from_live_response_sync(
        station: &Station,
        live: &LiveResponse,
        server_base_url: Option<&str>,
    ) -> Result<Self>;
```

C'est quoi exactement la fonction from_live_response_sync ?
J'ai l'impression que tu surcompliques encore.

```
pub fn to_didl_item(&self, parent_id: &str) -> Item 
```

Vu ma remarque précédente: 
  En fait, le didl de l'item, Dans notre nouvelle strategie est déjà un didl d'une playlist à un item.

Cette fonction devrait juste être un toDiddle et retourner le diddle d'une playlist à un item avec exactement les mêmes métadonnées pour la playlist conteneur et l'item à l'intérieur.

### 3. Cache de stations (intégré dans stateful_client.rs)

Comme je le disais plus haut, ce cache doit être permanent via l'usage de la configuration. Comme c'est le cas actuellement.

## Hiérarchie de browse

**Niveau 1**: Browse d'un groupe
En fait, si un station groupe ne contient qu'un seul item, C'est à dire qu'il n'y a pas plusieurs sous-radios sous ce groupe. on peut directement retourner la playlist simple qui contient simplement cet item. 

Questions pour validation

1. **Organisation des stations sans webradios**: Faut-il créer un groupe pour chaque station standalone (France Culture, France Inter, etc.) ou les mettre toutes dans un seul groupe "Stations principales"?
   - En fait, si un station groupe ne contient qu'un seul item, C'est à dire qu'il n'y a pas plusieurs sous-radios sous ce groupe. on peut directement retourner la playlist simple qui contient simplement cet item. Sinon, on retourne un container qui contient les playlists de chacun des items. Cela peut directement être implémenté dans le code de la fonction to Didl du groupe de station.

2. **Cache de métadonnées**: In-memory uniquement (données volatiles avec TTL court)?
   - Oui, in-memory seulement, TTL basé sur la fin de diffusion de cet item.

3. **Cache de stations**: Rester dans pmoconfig avec TTL 1 jour?
   - Oui, garder le système actuel, Il me semble que le TTL est d'une semaine actuellement, mais le garder tel qu'il est.

4. **Gestion d'erreur API Radio France down**: Retourner les données expirées avec warning?
   - **Proposition**: Oui, graceful degradation
     C'est parfait.
   
5. **Migration du code existant**: Faut-il maintenir une compatibilité temporaire ou refactoring complet immédiat?
   - **Proposition**: Refactoring complet, c'est une simplification profonde
     C'est parfait.


## Round 4

### 2. Groupes de stations (playlist.rs)

```
impl StationGroups {    
    // Browse niveau 0: retourne les containers de groupes
    pub async fn to_didl(&self, metadata_cache: &MetadataCache, server_base_url: &str) -> Vec<Container>;
}
```

Pourquoi retourner un vecteur de conteneurs et pas un conteneur qui contient des conteneurs? Ça doit retourner une structure didl La fonction s'appelle to_didl. 
    
  Il faut être cohérent. Et **SIMPLE**.
  
### 3. Cache de métadonnées (NOUVEAU: metadata_cache.rs)

Il y a actuellement dans le code des règles pour passer des métadonnées Radio France vers des métadonnées UPNP, qui agrège les métadonnées selon certaines règles depuis Radio France pour en faire des métadonnées plus simples mais avec une sémantique correcte pour l'interface utilisateur du côté UPNP. Il ne faut pas abandonner ces règles.

### Hiérarchie de browse

**Niveau 0**: `radiofrance` → containers de groupes  
- "France Culture" (id: `radiofrance:franceculture`) - playlist directe si groupe à 1 station
- "FIP" (id: `radiofrance:group:fip`) - container de groupe si plusieurs stations
- "Radios ICI" (id: `radiofrance:ici`) - container de groupe pour les radios locales --> Je te rappelle qu'il n'y a plus de distinction entre radio locale et autres radios. Ça c'était avant. Donc ICI fonctionne exactement comme FIP.

### Étapes d'implémentation

#### Étape 1: Créer metadata_cache.rs
1. Définir `CachedMetadata` struct avec tous les champs DIDL

On est d'accord que si tu définis ce type là, ça veut dire que tu supprimes le client Stateful. Sans ça, c'est complètement redondant.

A la fin de cette tâche, tu généreras le nouveau plan dans le fichier de rapport tel que c'est demandé par le fichier de règles [@Rules.md](file:///Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/Blackboard/Rules.md) que tu devras suivre scrupuleusement.
