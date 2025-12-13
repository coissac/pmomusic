# 🔍 Debugging Environment - START HERE

## Ce que nous savons

✅ **Python fonctionne** - Le script `test_qobuz.py` réussit à appeler `track_getFileUrl()`  
❌ **Rust échoue** - Erreur "Invalid Request Signature parameter (request_sig)"

**Conclusion:** Le problème est spécifique à notre implémentation Rust.

## Prochaine étape: Comparer les requêtes

Nous allons comparer exactement ce que Python envoie vs ce que Rust envoie.

### Option 1: Guide détaillé (recommandé)

Lisez [COMPARISON_GUIDE.md](COMPARISON_GUIDE.md) pour un guide étape par étape.

### Option 2: Script rapide

Terminal 1:
```bash
cd pmoqobuz/test_python_qobuz
python3 fake_qobuz_server.py
```

Terminal 2:
```bash
cd pmoqobuz/test_python_qobuz
./quick_compare.sh
```

## Fichiers disponibles

### Scripts de test:
- `test_qobuz.py` - Test complet contre l'API réelle (déjà validé ✅)
- `test_getfileurl.py` - Test simplifié pour comparaison avec fake server
- `fake_qobuz_server.py` - Serveur fake qui log toutes les requêtes
- `patch_for_fake.py` - Utilitaire pour rediriger vers fake server

### Guides:
- `COMPARISON_GUIDE.md` - Guide détaillé de comparaison
- `INSTRUCTIONS.md` - Instructions générales
- `README.md` - Documentation

### Scripts utilitaires:
- `quick_compare.sh` - Script automatique de comparaison
- `run_comparison.sh` - Alternative

## Ce qu'on cherche

En comparant les requêtes Python vs Rust pour `/track/getFileUrl`, on cherche:

1. **Format du timestamp** - Nombre de décimales?
2. **Ordre des paramètres** - Affecte-t-il la signature?
3. **Headers** - Content-Type manquant?
4. **Encoding** - Problème d'encodage du form data?

## Résultat attendu

Après comparaison, vous devriez identifier LA différence exacte qui cause l'échec de validation de signature côté Qobuz.

Exemple de différence possible:
```
Python:  request_ts=1734170123.456789  (6 décimales)
Rust:    request_ts=1734170123.45678   (5 décimales)
```

Cette petite différence suffirait à invalider la signature MD5!
