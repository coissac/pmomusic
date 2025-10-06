# Makefile pour projet Rust + Vue.js
# Variables de configuration
CARGO = cargo
NPM = npm
WEBAPP_DIR = pmoapp/webapp
DIST_DIR = $(WEBAPP_DIR)/dist
RUST_TARGET = target/release
DOC_DIR = target/doc
BINARY_NAME = PMOMusic

# Couleurs pour l'affichage
GREEN = \033[0;32m
YELLOW = \033[1;33m
RED = \033[0;31m
NC = \033[0m # No Color

.PHONY: all help build release debug test doc webapp clean install dev check fmt clippy watch

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
	$(CARGO) build --release
	@echo "$(GREEN)✓ Binaire disponible : $(RUST_TARGET)/$(BINARY_NAME)$(NC)"

## debug: Compile le binaire Rust en mode debug
debug: webapp
	@echo "$(YELLOW)→ Compilation Rust (debug)...$(NC)"
	$(CARGO) build
	@echo "$(GREEN)✓ Binaire disponible : target/debug/$(BINARY_NAME)$(NC)"

## test: Exécute tous les tests Rust
test:
	@echo "$(YELLOW)→ Exécution des tests Rust...$(NC)"
	$(CARGO) test --all
	@echo "$(GREEN)✓ Tests terminés$(NC)"

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

## bench: Exécute les benchmarks
bench:
	@echo "$(YELLOW)→ Exécution des benchmarks...$(NC)"
	$(CARGO) bench

## coverage: Génère un rapport de couverture de code
coverage:
	@echo "$(YELLOW)→ Génération du rapport de couverture...$(NC)"
	$(CARGO) tarpaulin --out Html --output-dir target/coverage
	@echo "$(GREEN)✓ Rapport disponible dans target/coverage/index.html$(NC)"
	