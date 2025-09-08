APP_NAME=pmomusic

# Build Go app
build: web
	go build -o bin/$(APP_NAME) ./cmd/$(APP_NAME)

# Build web frontend
web:
	@ echo Building the Web app
	@ cd pmoapp/web && npm install && npm run build
	@ echo Web app built

# Run Go app
run:
	go run ./cmd/$(APP_NAME)

# Clean Go binary and web build
clean:
	rm -rf bin/
	rm -rf web/dist

# Build everything
all: build
