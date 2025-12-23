# Test Python Qobuz - Debugging Suite

Ce répertoire contient des outils pour comparer le comportement Python vs Rust de l'API Qobuz.

## Fichiers

- `raw.py` - Module API Qobuz copié depuis UPMPdcli
- `spoofbuz.py` - Spoofer pour extraire les credentials
- `test_qobuz.py` - Script de test contre l'API réelle
- `fake_qobuz_server.py` - Serveur fake qui log toutes les requêtes
- `patch_for_fake.py` - Utilitaire pour rediriger vers le fake server

## Usage

### 1. Test contre l'API réelle Qobuz

```bash
cd pmoqobuz/test_python_qobuz
python3 test_qobuz.py
```

Entrer username et password quand demandé.

### 2. Test avec le fake server (pour debug)

Terminal 1 - Lancer le fake server:
```bash
cd pmoqobuz/test_python_qobuz
python3 fake_qobuz_server.py
```

Terminal 2 - Patcher et tester:
```bash
cd pmoqobuz/test_python_qobuz
python3 patch_for_fake.py          # Redirige vers localhost:8080
python3 test_qobuz.py               # Lance le test
python3 patch_for_fake.py restore   # Restaure l'URL originale
```

Le fake server affichera TOUS les détails des requêtes:
- Méthode HTTP (GET/POST)
- URL complète
- Headers
- Query parameters (si GET)
- Form data (si POST)
- Timestamp exact de chaque requête

## Comparaison Python vs Rust

Pour comparer:
1. Lancer le fake server
2. Patcher raw.py
3. Lancer test_qobuz.py → Observer les logs du serveur
4. Restaurer raw.py
5. Modifier le code Rust pour pointer vers localhost:8080
6. Lancer l'exemple Rust → Observer les logs du serveur
7. Comparer les deux sorties ligne par ligne

## Ce qu'on cherche

- Ordre des paramètres dans la signature
- Format exact du timestamp
- Différences dans les headers
- Différences POST vs GET
- Format exact de request_sig
