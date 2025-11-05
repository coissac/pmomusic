# Configuration Sécurisée

## Configuration de PMOMusic

Le fichier `.pmomusic.yml` contient des informations sensibles (mots de passe, identifiants).

### Installation

1. Copiez le fichier exemple :
   ```bash
   cp .pmomusic.yml.example .pmomusic.yml
   ```

2. Éditez `.pmomusic.yml` et remplacez les valeurs par vos véritables identifiants :
   - `accounts.qobuz.username` : votre email Qobuz
   - `accounts.qobuz.password` : votre mot de passe Qobuz

3. **Important** : Ne commitez JAMAIS le fichier `.pmomusic.yml` dans git !
   - Il est déjà dans `.gitignore`
   - Utilisez des variables d'environnement pour la production

## Variables d'environnement (recommandé pour production)

```bash
export QOBUZ_USERNAME="votre-email@example.com"
export QOBUZ_PASSWORD="votre-mot-de-passe"
```
