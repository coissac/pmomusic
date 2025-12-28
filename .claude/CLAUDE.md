# PMOMusic Project Configuration

## Version Control
Ce projet utilise **Jujutsu (jj)** pour le contrôle de version, PAS git.
- Utiliser les commandes `jj` au lieu des commandes `git`
- Bookmark principal : `main`
- Ne jamais suggérer de commandes git

## Environnement
Le PATH et les variables d'environnement sont configurés dans `.claude-env` à la racine du projet.

## Configuration de l'application
- Fichier de configuration principal : `.pmomusic/config.yaml`
- Configuration UPNP personnalisable pour différencier les instances en développement

## Développement
Pendant le développement, plusieurs serveurs PMOMusic peuvent tourner en parallèle. Utiliser la configuration UPNP dans `.pmomusic/config.yaml` pour différencier les instances :

```yaml
host:
  upnp:
    manufacturer: "PMOMusic-Dev1"
    udn_prefix: "pmomusic-dev1"
    model_name_prefix: "PMOMusic-Dev1"
    friendly_name_prefix: "PMOMusic-Dev1"
```

## Architecture
- Projet Rust multi-crates avec workspaces
- Crates principales : pmoupnp, pmomediaserver, pmomediarenderer, pmoconfig
- Pattern d'extension de configuration via traits (voir pmocache/src/config_ext.rs)
