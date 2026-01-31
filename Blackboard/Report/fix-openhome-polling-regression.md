# Rapport : Correction de la régression du polling OpenHome

## Résumé

Suite au crash de Claude Code, investigation et correction d'une régression causant des sauts de 2-3 secondes dans la barre de progression de l'interface web pour les renderers OpenHome. Le problème provenait d'une combinaison de facteurs : timing incorrect de la boucle de polling et appels SOAP redondants.

## Fichiers modifiés

1. `pmocontrol/src/music_renderer/musicrenderer.rs`
   - Correction du timing de la boucle watcher (intervalle fixe au lieu de pause fixe)
   - Suppression d'un appel double à `playback_position()`
   - Réorganisation de `poll_and_emit_changes()` pour minimiser le temps passé avec les locks

2. `pmocontrol/src/music_renderer/openhome_renderer.rs`
   - Ajout d'un cache intelligent pour `playback_position()` avec timestamp et détection d'abus
   - Évite les appels SOAP redondants (OpenHome a précision à la seconde)

3. `pmocontrol/src/music_renderer/watcher.rs`
   - Modifications temporaires annulées (cache déplacé dans OpenHomeRenderer)

## Analyse des appels SOAP OpenHome - Services de LECTURE

Analyse effectuée sur le renderer OpenHome `pizzicato-Music` (192.168.0.200) à partir des logs `pmomusic.log`.

### Services analysés et intervalles observés

#### ✅ Time:Time (après correction)
- **Intervalle moyen** : ~1050ms
- **Min/Max** : 1000-1200ms
- **État** : CORRIGÉ - Cache actif, fonctionne parfaitement
- **Appels** : Réguliers, espacés d'environ 1 seconde

#### ⚠️ Playlist:TransportState
- **Intervalle moyen** : ~150ms
- **Distribution** : 
  - 100ms : 7 occurrences
  - 200ms : 1 occurrence
  - 700ms : 1 occurrence
- **État** : PROBLÉMATIQUE - Sur-sollicitation
- **Impact** : Appelé 6-7 fois par seconde au lieu de 2 fois

#### ⚠️ Playlist:IdArray
- **Intervalle moyen** : ~320ms (très irrégulier)
- **Distribution** :
  - 0ms : 2 occurrences (!)
  - 100ms : 3 occurrences
  - 200ms : 1 occurrence
  - 800-1000ms : 3 occurrences
- **État** : TRÈS PROBLÉMATIQUE - Appels anarchiques
- **Impact** : Certains appels consécutifs sans délai, surcharge réseau

#### ⚠️ Product:SourceXml
- **Intervalle moyen** : ~130ms
- **Distribution** :
  - 100ms : 8 occurrences
  - 200ms : 1 occurrence
  - 300ms : 1 occurrence
- **État** : PROBLÉMATIQUE - Sur-sollicitation
- **Impact** : Appelé 7-8 fois par seconde au lieu de 2 fois

#### ⚠️ Product:SourceIndex
- **Données** : Observé dans les logs mais pas analysé en détail
- **État** : Probablement similaire à SourceXml

#### 📊 Volume:Volume & Volume:Mute
- **Données** : Insuffisantes dans les logs récents
- **Polling prévu** : Toutes les 2 ticks (1 seconde) selon le code
- **État** : À surveiller

### Services d'ÉCRITURE

Aucun appel récent observé dans les logs (comportement normal - ce sont des commandes utilisateur ponctuelles) :
- Playlist:Play
- Playlist:Pause  
- Playlist:Stop
- Playlist:SeekId
- Playlist:SeekSecondAbsolute
- Volume:SetVolume
- Volume:SetMute

## Problèmes identifiés

### 1. Timing de la boucle watcher (CORRIGÉ)
**Avant** : `sleep(500ms)` APRÈS chaque poll
- Poll prend 100-200ms → Intervalle réel = 600-700ms

**Après** : Intervalle fixe de 500ms entre le DÉBUT de chaque poll
- Utilise `SystemTime` pour calculer le prochain poll
- Ajuste le sleep en conséquence

### 2. Lock contention (CORRIGÉ)
**Avant** : Lock `watched_state` tenu pendant les appels réseau
- Bloque autres threads pendant 50-200ms
- Cause des délais cumulatifs

**Après** : Locks acquis uniquement pour comparaison/mise à jour
- Appels réseau faits SANS locks
- Locks relâchés avant émission d'événements

### 3. Appels SOAP redondants OpenHome:Time (CORRIGÉ)
**Avant** : Aucun cache, appel SOAP à chaque poll (500ms)
- OpenHome retourne `elapsed_secs` (précision seconde)
- Appels inutiles car valeur identique

**Après** : Cache avec expiration 900ms + détection d'abus
- Retourne valeur cachée si < 900ms
- Warning si > 3 appels/seconde
- Réduit appels SOAP de moitié

### 4. Appel double à playback_position() (CORRIGÉ)
**Avant** : Deux appels dans `poll_and_emit_changes()`
```rust
let raw_position = self.lock_backend_for("poll_position").playback_position().ok();
let position = self.playback_position().ok();
```

**Après** : Un seul appel
```rust
let position = self.playback_position().ok();
```

## Problèmes restants (NON CORRIGÉS)

### Services OpenHome sur-sollicités

Les services suivants sont appelés trop fréquemment (100-300ms au lieu de 500ms+) :
- **Playlist:TransportState** (~150ms) - utilisé par `playback_state()`
- **Playlist:IdArray** (~320ms, irrégulier) - utilisé par les opérations de queue
- **Product:SourceXml** (~130ms) - vérification de source active
- **Product:SourceIndex** (non mesuré) - probablement similaire

**Impact** :
- Surcharge réseau inutile
- Potentiel de ralentissement avec latence réseau élevée
- Gaspillage CPU (parsing SOAP)

**Solution recommandée** :
Appliquer le même pattern de cache qu'on a fait pour `Time:Time` à ces méthodes :
- `playback_state()` → cache TransportState
- Méthodes de queue → cache IdArray  
- Vérification de source → cache SourceXml/SourceIndex

## Tests et validation

- Compilation : ✅ Succès (15:38 heure de Paris)
- Logs analysés : `pmomusic.log` (14:54 UTC = 15:54 Paris)
- Barre de progression : ✅ Fluide (confirmé par utilisateur)
- Appels Time : ✅ Espacés de ~1s (au lieu de 0.6-1.8s avant)
- Warnings abus : ✅ Aucun (< 3 appels/seconde)

## Conclusion

La régression de la barre de progression est corrigée. Le service `Time` bénéficie maintenant d'un cache intelligent qui évite les appels redondants. Cependant, l'analyse des logs révèle que d'autres services OpenHome souffrent du même problème de sur-sollicitation et mériteraient le même traitement.

## Métriques

- Temps d'investigation : ~2h (après crash)
- Crates modifiés : `pmocontrol`
- Lignes modifiées : ~150 (ajouts + suppressions)
- Services corrigés : 1/5 identifiés
