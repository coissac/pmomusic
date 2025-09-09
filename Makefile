APP_NAME=pmomusic

PREFIX := $(CURDIR)/C
SRC_DIR := $(PREFIX)/src/soxr-0.1.3
INCLUDE_DIR := $(PREFIX)/include
LIB_DIR := $(PREFIX)/lib
BIN_DIR := $(PREFIX)/bin
CFLAGS = -O2 -fPIC
LDFLAGS :=
LIBSOXR := $(LIB_DIR)/libsoxr.a

.PHONY: all clean build web run test

# Build Go app
build: web $(LIBSOXR)
	go build -o bin/$(APP_NAME) ./cmd/$(APP_NAME)

# Build web frontend
web:
	@ echo Building the Web app
	@ cd pmoapp/web && npm install && npm run build
	@ echo Web app built

# Run Go app
run:
	go run ./cmd/$(APP_NAME)

# Clean Go binary, web build, libsoxr build
clean:
	rm -rf bin/$(APP_NAME)
	rm -rf web/dist
	@echo "==> Cleaning libsoxr build..."
	# supprime les fichiers compilés libsoxr
	rm -rf $(INCLUDE_DIR)/* $(LIB_DIR)/*
	@echo "==> Clean done"

# Compilation statique de libsoxr
$(LIBSOXR):
	@echo "==> Building libsoxr statically..."
	mkdir -p $(INCLUDE_DIR) $(LIB_DIR) $(BIN_DIR)
	cd $(SRC_DIR) && ./go
	# Installer dans PREFIX (headers et lib)
	cd $(SRC_DIR)/Release && make install PREFIX=$(PREFIX)
	@echo "==> libsoxr built: $(LIBSOXR)"

# Build everything
all: build

# Run Go tests (requires libsoxr built)
test: $(LIBSOXR)
	CGO_ENABLED=1 go test ./...
