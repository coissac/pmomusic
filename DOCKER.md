# Docker Deployment Guide for PMOMusic

Ce guide explique comment construire et déployer PMOMusic avec Docker.

## Architecture

Le Dockerfile utilise une approche multi-stage pour créer une image minimale :

1. **Stage 1 (webapp-builder)** : Compile l'application Vue.js avec Node.js
2. **Stage 2 (rust-builder)** : Compile le binaire Rust avec toutes ses dépendances
3. **Stage 3 (runtime)** : Image finale minimale Debian Slim avec uniquement le binaire et les bibliothèques runtime

### Avantages

- **Binaire auto-contenu** : L'application web est embarquée dans le binaire Rust
- **Image minimale** : ~200-300MB (vs plusieurs GB pour les images de build)
- **Sécurité** : Exécution en tant qu'utilisateur non-root
- **Reproductibilité** : Build complet et déterministe

## Build de l'image

### Option 1 : Build manuel avec Docker

```bash
# Build l'image
docker build -t pmomusic:latest .

# Le build prend environ 10-15 minutes selon votre machine
```

### Option 2 : Build avec docker-compose

```bash
# Build et démarre le conteneur
docker-compose up --build

# Ou juste build
docker-compose build
```

### Build optimisé avec cache

Pour accélérer les builds successifs, Docker réutilise les couches en cache :

```bash
# Build avec cache
docker build -t pmomusic:latest .

# Build sans cache (force rebuild complet)
docker build --no-cache -t pmomusic:latest .
```

## Exécution du conteneur

### Option 1 : Avec docker-compose (recommandé)

```bash
# Démarrer en arrière-plan
docker-compose up -d

# Voir les logs
docker-compose logs -f

# Arrêter
docker-compose down

# Redémarrer
docker-compose restart
```

### Option 2 : Avec docker run

```bash
# Run en mode interactif
docker run -it --rm \
  --name pmomusic \
  --network host \
  -v $(pwd)/config:/home/pmomusic/.pmomusic \
  -v $(pwd)/cache:/home/pmomusic/cache \
  pmomusic:latest

# Run en mode détaché
docker run -d \
  --name pmomusic \
  --network host \
  --restart unless-stopped \
  -v $(pwd)/config:/home/pmomusic/.pmomusic \
  -v $(pwd)/cache:/home/pmomusic/cache \
  pmomusic:latest
```

## Configuration

### Ports

Par défaut, PMOMusic écoute sur le port **8080**. Vous pouvez modifier cela :

- Dans `docker-compose.yml` : modifier la section `ports`
- Avec `docker run` : utiliser `-p 8080:8080`

### Volumes

Deux volumes sont recommandés pour la persistance :

- **Configuration** : `/home/pmomusic/.pmomusic` - Fichiers de configuration
- **Cache** : `/home/pmomusic/cache` - Cache audio et métadonnées

### Variables d'environnement

Configurable via `docker-compose.yml` ou `-e` avec `docker run` :

```bash
# Niveau de logs Rust
RUST_LOG=debug

# Autres variables (selon votre configuration)
# ...
```

### Réseau

Pour UPnP/DLNA, utilisez **network_mode: host** pour permettre :
- La découverte multicast
- La communication avec les devices UPnP sur le réseau local

**Note** : Le mode `host` ne fonctionne que sur Linux. Sur macOS/Windows avec Docker Desktop, utilisez le mapping de ports standard.

## Gestion de l'image

### Taille de l'image

```bash
# Voir la taille de l'image
docker images pmomusic:latest

# Résultat attendu : ~200-300MB
```

### Nettoyage

```bash
# Supprimer l'image
docker rmi pmomusic:latest

# Nettoyer les images de build intermédiaires
docker builder prune

# Nettoyer tous les caches Docker (libère beaucoup d'espace)
docker system prune -a
```

## Build multi-plateforme

Pour builder pour différentes architectures (ARM64, AMD64) :

```bash
# Créer un builder multi-plateforme
docker buildx create --name multiarch --use

# Build pour AMD64 et ARM64
docker buildx build \
  --platform linux/amd64,linux/arm64 \
  -t pmomusic:latest \
  --push \
  .

# Note : nécessite un registry Docker (Docker Hub, GHCR, etc.)
```

## Déploiement en production

### 1. Avec docker-compose (simple)

```bash
# Sur le serveur de production
git clone <votre-repo>
cd pmomusic
docker-compose up -d
```

### 2. Avec un registry Docker (recommandé)

```bash
# Sur votre machine de dev
docker build -t yourregistry.com/pmomusic:v1.0.0 .
docker push yourregistry.com/pmomusic:v1.0.0

# Sur le serveur de production
docker pull yourregistry.com/pmomusic:v1.0.0
docker run -d ... yourregistry.com/pmomusic:v1.0.0
```

### 3. Avec un orchestrateur (Kubernetes, Docker Swarm)

Créer un fichier de déploiement approprié selon votre orchestrateur.

## Debugging

### Logs du conteneur

```bash
# Logs en temps réel
docker logs -f pmomusic

# Logs avec docker-compose
docker-compose logs -f
```

### Entrer dans le conteneur

```bash
# Shell interactif (bash n'est pas disponible, utiliser sh)
docker exec -it pmomusic sh

# Vérifier les processus
docker exec -it pmomusic ps aux

# Vérifier les fichiers
docker exec -it pmomusic ls -la /home/pmomusic
```

### Health check

Le conteneur inclut un health check. Vérifier l'état :

```bash
# Voir l'état de santé
docker inspect --format='{{.State.Health.Status}}' pmomusic
```

## Troubleshooting

### Le build échoue

1. **Erreur de dépendances npm** :
   - Vérifier que `pmoapp/webapp/package.json` est correct
   - Essayer `docker build --no-cache`

2. **Erreur de compilation Rust** :
   - Vérifier que tous les fichiers Cargo.toml sont présents
   - Vérifier les dépendances système (libsoxr, libasound2)

3. **Out of memory** :
   - Augmenter la mémoire allouée à Docker Desktop (settings)
   - Utiliser `--memory` pour limiter la mémoire du build

### Le conteneur ne démarre pas

1. **Port déjà utilisé** :
   ```bash
   # Vérifier quel processus utilise le port 8080
   sudo lsof -i :8080
   ```

2. **Permissions** :
   - Vérifier les permissions des volumes montés
   - Le conteneur s'exécute en tant qu'utilisateur `pmomusic` (UID 1000)

3. **Configuration manquante** :
   - Créer les répertoires de configuration avant de démarrer :
     ```bash
     mkdir -p config cache
     ```

### UPnP ne fonctionne pas

1. **Network mode** :
   - Sur Linux : utiliser `network_mode: host`
   - Sur macOS/Windows : UPnP peut ne pas fonctionner correctement avec Docker Desktop

2. **Firewall** :
   - Vérifier que les ports UPnP ne sont pas bloqués
   - Autoriser le multicast sur le réseau

## Performance

### Optimisations du build

1. **Build cache** : Docker réutilise les couches en cache
2. **Multi-stage build** : Réduit la taille de l'image finale
3. **Strip des symboles** : Le binaire est strippé pour réduire sa taille

### Optimisations runtime

1. **Resource limits** : Définir des limites CPU/mémoire dans docker-compose.yml
2. **Volumes** : Utiliser des volumes pour les données persistantes
3. **Logs** : Configurer la rotation des logs Docker

## Sécurité

- ✅ Exécution en tant qu'utilisateur non-root
- ✅ Image minimale (surface d'attaque réduite)
- ✅ Pas de secrets dans l'image
- ✅ Health checks activés
- ✅ Certificats CA inclus pour HTTPS

## Références

- [Dockerfile](./Dockerfile)
- [docker-compose.yml](./docker-compose.yml)
- [.dockerignore](./.dockerignore)
- [Documentation Rust](./Readme.md)
