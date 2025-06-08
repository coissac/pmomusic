APP_NAME=pmomusic

build:
	go build -o bin/$(APP_NAME) ./cmd/$(APP_NAME)

run:
	go run ./cmd/$(APP_NAME)

clean:
	rm -rf bin/

all: build