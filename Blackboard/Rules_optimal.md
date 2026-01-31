# PMOMusic - Règles LLM (IMPÉRATIF)

## 🎯 Projet
Audio HiFi UPnP/DLNA. Backend Rust, Frontend Vue.js/TypeScript.

**Environnement** : `source .claude-env` (racine projet) AVANT toute commande.

---

## ⛔ INTERDICTIONS (0 EXCEPTION)

1. **JAMAIS déplacer/créer fichiers** dans `Blackboard/` (seul humain décide)
2. **JAMAIS commencer** sans crates explicites dans `Todo/{nom}.md` → REFUSER
3. **JAMAIS compiler/tester** (`cargo`, `npm`) → TOUJOURS demander à humain
4. **JAMAIS détailler** implémentation dans discussion → UN message : "Tâche terminée. Voir `Report/{nom}.md`..."

---

## 📋 WORKFLOW (STRICT)

```
1. LIRE Todo/{nom}.md
   Crates spécifiées ? NON → ARRÊTER, demander | OUI → Continuer
   
2. IMPLÉMENTER
   Patterns Architecture/ si référencés
   DEMANDER compilation : "Compilez `cargo build -p {crate}`, renvoyez erreurs"
   Erreurs ? OUI → Corriger, redemander | NON → Continuer
   
3. CRÉER Report/{nom}.md
   - Résumé (2-3 phrases, SANS code/détails techniques)
   - Fichiers modifiés (chemins complets)
   - Modifications SÉMANTIQUES (concepts, PAS lignes code)
   ÉCRIRE dans chat : "Tâche terminée. Voir `Report/{nom}.md`..."
   ARRÊTER (ne rien déplacer)
   
4. SI humain déplace Todo/{nom}.md → Done/{nom}.md
   ALORS écrire synthèse COMPLÈTE dans Done/{nom}.md
```

---

## 🔧 RÈGLES TECHNIQUES

**Dépendances** : TOUJOURS workspace (`Cargo.toml` racine) sauf exception justifiée
```toml
# ✅ workspace.dependencies puis { workspace = true }
# ❌ version directe dans crate
```

---

## 📂 BLACKBOARD

| Dossier | LLM crée | LLM déplace | Humain déplace |
|---------|----------|-------------|----------------|
| `Todo/` | ❌ | ❌ | ✅ → Done/ToDiscuss |
| `Report/` | ✅ | ❌ | ❌ |
| `Done/` | ❌ (écrit après déplacement) | ❌ | ✅ |
| `ToDiscuss/` | ❌ | ❌ | ✅ |

---

## ✅ CHECKLIST

**Avant** :
- [ ] `source .claude-env`
- [ ] Crates dans `Todo/{nom}.md` ? NON → ARRÊTER

**Pendant** :
- [ ] Patterns existants
- [ ] Workspace dependencies
- [ ] NE PAS compiler

**Après** :
- [ ] `Report/{nom}.md` : résumé court + fichiers + modifs sémantiques (SANS code)
- [ ] Chat : "Tâche terminée. Voir `Report/{nom}.md`..." (RIEN d'autre)
- [ ] NE PAS déplacer `Todo/{nom}.md`

---

## 📝 TEMPLATES

### Report/{nom}.md
```markdown
# Rapport : {titre}

## Résumé
{2-3 phrases SANS code}

## Fichiers modifiés
1. `chemin/fichier.rs`
   - {Modification sémantique 1}
   - {Modification sémantique 2}
```

**Modif sémantique** = concept (ex: "Ajout cache"), PAS ligne code (ex: ❌ "Ajout `let x = 5;`")

### Discussion
```
Tâche terminée. Voir `Report/{nom}.md` pour la liste des modifications.
```

### Done/{nom}.md (après déplacement humain)
```markdown
# {Titre}
## Spécification
{Copie Todo/ complète}
## Implémentation
{Détails complets par fichier}
## Tests/Validation
## Conclusion
```

---

## 🎯 6 RÈGLES D'OR

1. JAMAIS déplacer fichiers Blackboard
2. EXIGER crates dans Todo/ (sinon REFUSER)
3. JAMAIS compiler (demander humain)
4. Report court SANS code/détails
5. Discussion : 1 ligne après implémentation
6. Done/ : écrire APRÈS déplacement humain

---

## 🔍 AUTO-VÉRIF (chaque message)

- [ ] Déplacé fichier ? → ERREUR
- [ ] >2 lignes chat après implémentation ? → ERREUR  
- [ ] Commencé sans vérif crates ? → ERREUR
- [ ] Compilé moi-même ? → ERREUR
- [ ] Créé Done/ ? → ERREUR
- [ ] Supposé code compile ? → ERREUR

**ERREUR détectée** → ARRÊTER immédiatement
