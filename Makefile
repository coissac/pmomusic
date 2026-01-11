# Makefile pour projet Rust + Vue.js
# Variables de configuration
CARGO = cargo
CARGO_NIGHTLY = rustup run nightly cargo
FEATURES ?=
NPM = npm
WEBAPP_DIR = pmoapp/webapp
DIST_DIR = $(WEBAPP_DIR)/dist
RUST_TARGET = target/release
DOC_DIR = target/doc
BINARY_NAME = PMOMusic

# Couleurs pour l'affichage
GREEN = \033[0;32m
YELLOW = \033[1;33m
BLUE = \033[1;34m
RED = \033[0;31m
NC = \033[0m # No Color

.DEFAULT_GOAL := simd

.PHONY: all help build release debug test doc webapp clean install dev check fmt clippy watch simd scalar

# Cible par défaut
all: build

## help: Affiche cette aide
help:
	@echo "$(GREEN)Commandes disponibles :$(NC)"
	@sed -n 's/^##//p' $(MAKEFILE_LIST) | column -t -s ':' | sed -e 's/^/ /'

## build: Compile tout (Rust + Vue.js)
build: webapp release
	@echo "$(GREEN)✓ Build complet terminé$(NC)"

## release: Compile le binaire Rust en mode release
release: webapp
	@echo "$(YELLOW)→ Compilation Rust (release)...$(NC)"
	$(CARGO) build --release $(FEATURES)
	@echo "$(GREEN)✓ Binaire disponible : $(RUST_TARGET)/$(BINARY_NAME)$(NC)"

## debug: Compile le binaire Rust en mode debug
debug: webapp
	@echo "$(YELLOW)→ Compilation Rust (debug)...$(NC)"
	$(CARGO) build $(FEATURES)
	@echo "$(GREEN)✓ Binaire disponible : target/debug/$(BINARY_NAME)$(NC)"

## test: Exécute tous les tests Rust
test:
	@echo "$(YELLOW)→ Exécution des tests Rust...$(NC)"
	$(CARGO) test --all
	@echo "$(GREEN)✓ Tests terminés$(NC)"

## simd: Compile l'application en mode SIMD (nightly requis)
simd:
	@echo "$(YELLOW)→ Build SIMD (nightly)...$(NC)"
	$(MAKE) release CARGO="$(CARGO_NIGHTLY)" FEATURES="--features simd"
	@echo "$(GREEN)✓ Build SIMD terminé$(NC)"

## scalar: Compile l'application en mode scalaire
scalar:
	@echo "$(YELLOW)→ Build scalaire...$(NC)"
	$(MAKE) release FEATURES=""
	@echo "$(GREEN)✓ Build scalaire terminé$(NC)"

## test-doc: Teste les exemples dans la documentation
test-doc:
	@echo "$(YELLOW)→ Test des exemples de documentation...$(NC)"
	$(CARGO) test --doc
	@echo "$(GREEN)✓ Tests de documentation terminés$(NC)"

## doc: Génère et ouvre la documentation Rust
doc:
	@echo "$(YELLOW)→ Génération de la documentation...$(NC)"
	$(CARGO) doc --no-deps --document-private-items --open
	@echo "$(GREEN)✓ Documentation générée dans $(DOC_DIR)$(NC)"

## doc-build: Génère la documentation sans l'ouvrir
doc-build:
	@echo "$(YELLOW)→ Génération de la documentation...$(NC)"
	$(CARGO) doc --no-deps --document-private-items
	@echo "$(GREEN)✓ Documentation générée dans $(DOC_DIR)$(NC)"

## webapp: Compile l'application Vue.js
webapp: webapp-install
	@echo "$(YELLOW)→ Build Vue.js...$(NC)"
	cd $(WEBAPP_DIR) && $(NPM) run build
	@echo "$(GREEN)✓ Application Vue.js compilée dans $(DIST_DIR)$(NC)"

## webapp-install: Installe les dépendances npm
webapp-install:
	@echo "$(YELLOW)→ Installation des dépendances npm...$(NC)"
	cd $(WEBAPP_DIR) && $(NPM) install
	@echo "$(GREEN)✓ Dépendances npm installées$(NC)"

## webapp-dev: Lance le serveur de développement Vue.js
webapp-dev:
	@echo "$(YELLOW)→ Démarrage du serveur de dev Vue.js...$(NC)"
	cd $(WEBAPP_DIR) && $(NPM) run dev

## clean: Nettoie les fichiers de build
clean:
	@echo "$(YELLOW)→ Nettoyage...$(NC)"
	$(CARGO) clean
	rm -rf $(DIST_DIR)
	rm -rf $(WEBAPP_DIR)/node_modules
	@echo "$(GREEN)✓ Nettoyage terminé$(NC)"

## clean-rust: Nettoie uniquement les builds Rust
clean-rust:
	@echo "$(YELLOW)→ Nettoyage Rust...$(NC)"
	$(CARGO) clean
	@echo "$(GREEN)✓ Nettoyage Rust terminé$(NC)"

## clean-webapp: Nettoie uniquement le build Vue.js
clean-webapp:
	@echo "$(YELLOW)→ Nettoyage Vue.js...$(NC)"
	rm -rf $(DIST_DIR)
	@echo "$(GREEN)✓ Nettoyage Vue.js terminé$(NC)"

## install: Installe le binaire dans ~/.cargo/bin
install: release
	@echo "$(YELLOW)→ Installation du binaire...$(NC)"
	$(CARGO) install --path .
	@echo "$(GREEN)✓ $(BINARY_NAME) installé$(NC)"

## dev: Lance le serveur en mode debug (recompile à chaque changement)
dev:
	@echo "$(YELLOW)→ Démarrage en mode développement...$(NC)"
	$(CARGO) watch -x run

## check: Vérifie que le code compile sans générer de binaire
check:
	@echo "$(YELLOW)→ Vérification du code...$(NC)"
	$(CARGO) check --all
	@echo "$(GREEN)✓ Code valide$(NC)"

## fmt: Formate le code Rust
fmt:
	@echo "$(YELLOW)→ Formatage du code...$(NC)"
	$(CARGO) fmt --all
	@echo "$(GREEN)✓ Code formaté$(NC)"

## fmt-check: Vérifie le formatage sans modifier
fmt-check:
	@echo "$(YELLOW)→ Vérification du formatage...$(NC)"
	$(CARGO) fmt --all -- --check

## clippy: Exécute clippy (linter Rust)
clippy:
	@echo "$(YELLOW)→ Analyse avec clippy...$(NC)"
	$(CARGO) clippy --all-targets --all-features -- -D warnings
	@echo "$(GREEN)✓ Analyse clippy terminée$(NC)"

## watch: Recompile automatiquement à chaque changement
watch:
	@echo "$(YELLOW)→ Mode watch activé...$(NC)"
	$(CARGO) watch -x check

## ci: Exécute toutes les vérifications CI
ci: fmt-check clippy test doc-build webapp
	@echo "$(GREEN)✓ Toutes les vérifications CI passées$(NC)"

## run: Lance le binaire en mode debug
run: debug
	@echo "$(YELLOW)→ Lancement de l'application...$(NC)"
	./target/debug/$(BINARY_NAME)

## run-release: Lance le binaire en mode release
run-release: release
	@echo "$(YELLOW)→ Lancement de l'application (release)...$(NC)"
	./$(RUST_TARGET)/$(BINARY_NAME)

## size: Affiche la taille du binaire
size:
	@echo "$(YELLOW)Taille des binaires :$(NC)"
	@if [ -f "target/debug/$(BINARY_NAME)" ]; then \
		echo "  Debug:   $$(du -h target/debug/$(BINARY_NAME) | cut -f1)"; \
	fi
	@if [ -f "$(RUST_TARGET)/$(BINARY_NAME)" ]; then \
		echo "  Release: $$(du -h $(RUST_TARGET)/$(BINARY_NAME) | cut -f1)"; \
	fi

## deps: Liste les dépendances obsolètes
deps:
	@echo "$(YELLOW)→ Vérification des dépendances...$(NC)"
	$(CARGO) outdated

## update: Met à jour les dépendances
update:
	@echo "$(YELLOW)→ Mise à jour des dépendances Rust...$(NC)"
	$(CARGO) update
	@echo "$(YELLOW)→ Mise à jour des dépendances npm...$(NC)"
	cd $(WEBAPP_DIR) && $(NPM) update
	@echo "$(GREEN)✓ Dépendances mises à jour$(NC)"

## bump-version: Incrémente le numéro de version patch (x.y.z -> x.y.z+1)
bump-version:
	@echo "$(YELLOW)→ Incrémentation de la version...$(NC)"
	@current=$$(grep '^version = ' PMOMusic/Cargo.toml | head -n 1 | sed 's/version = "\(.*\)"/\1/'); \
	echo "  Version actuelle: $$current"; \
	major=$$(echo $$current | cut -d. -f1); \
	minor=$$(echo $$current | cut -d. -f2); \
	patch=$$(echo $$current | cut -d. -f3); \
	new_patch=$$((patch + 1)); \
	new_version="$$major.$$minor.$$new_patch"; \
	echo "  Nouvelle version: $$new_version"; \
	sed -i.bak "s/^version = \"$$current\"/version = \"$$new_version\"/" PMOMusic/Cargo.toml && \
	rm PMOMusic/Cargo.toml.bak
	@echo "$(GREEN)✓ Version mise à jour dans PMOMusic/Cargo.toml$(NC)"

## bench: Exécute les benchmarks
bench:
	@echo "$(YELLOW)→ Exécution des benchmarks...$(NC)"
	$(CARGO) bench

## coverage: Génère un rapport de couverture de code
coverage:
	@echo "$(YELLOW)→ Génération du rapport de couverture...$(NC)"
	$(CARGO) tarpaulin --out Html --output-dir target/coverage
	@echo "$(GREEN)✓ Rapport disponible dans target/coverage/index.html$(NC)"

jjnew:
	@echo "$(YELLOW)→ Création d'un nouveau commit...$(NC)"
	@echo "$(BLUE)→ Documentation du commit courrant...$(NC)"
	@jj auto-describe
	@echo "$(BLUE)→ C'est fait.$(NC)"
	@jj new
	@echo "$(GREEN)✓ nouveau commit créé$(NC)"

jjpush: bump-version
	@echo "$(YELLOW)→ Push du commit sur le dépôt...$(NC)"
	@jj auto-describe
	@jj git push --change @
	@echo "$(GREEN)✓ Commit pushé sur le dépôt$(NC)"

jjfetch:
	@echo "$(YELLOW)→ Pull des derniers commits...$(NC)"
	@jj git fetch
	@jj new main@origin
	@echo "$(GREEN)✓ Derniers commits pullés$(NC)"
