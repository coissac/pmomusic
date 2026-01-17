# Tâche : Amélioration de l'overlay de couverture d'album

**Crate concernée** : pmoapp/webapp

**Statut** : Terminée

---

## Demande initiale

### Point 1 : Métadonnées et progress bar
Quand on clique sur l'image de couverture de l'album, elle s'agrandit pour prendre tout l'espace. Par défaut les métadonnées sont cachées et si on clique sur l'écran, il y a un effet toggle. Il fallait que par défaut elles soient visibles, avec une progress bar draggable dans une petite boîte au fond gris transparent avec un effet glass metal à la macOS.

### Point 2 : Ergonomie mobile
Le bouton de fermeture en haut à droite n'était pas ergonomique sur mobile où les doigts sont plutôt en bas de l'écran.

---

## Synthèse des modifications

### Fichier modifié
- `pmoapp/webapp/src/components/pmocontrol/CurrentTrack.vue`

### Point 1 : Résolu
- Métadonnées visibles par défaut à l'ouverture
- Progress bar interactive ajoutée dans le panneau des métadonnées (souris + tactile)
- Style glassmorphism : fond transparent 35%, blur 30px, bordure subtile, ombre
- Panneau réduit à 70% de largeur pour un look plus élégant

### Point 2 : Résolu
- **Swipe down** pour fermer l'overlay sur tactile (seuil 100px)
- Feedback visuel pendant le geste (translation + fade)
- Bouton X conservé pour desktop
- Blocage du pull-to-refresh Android (`overscroll-behavior: contain`)

### Corrections additionnelles
- Support tactile de la progress bar principale (hors overlay)
- Utilisation de `100dvh` au lieu de `100vh` pour compatibilité avec les barres de navigation mobiles (Brave, Safari, Chrome)
- Corrections TypeScript pour les événements tactiles

---

## Éléments techniques clés

### Swipe down
```typescript
const swipeStartY = ref(0);
const swipeCurrentY = ref(0);
const isSwiping = ref(false);
const swipeThreshold = 100;
```
Listeners ajoutés sur `document` pour capturer le mouvement même sur les éléments enfants.

### CSS mobile
```css
.cover-overlay {
    overscroll-behavior: contain;
    touch-action: none;
}
.cover-overlay-content {
    height: calc(100dvh - 32px);
}
```
