# Makefile for qtile-cmd-client (qticc)

BINARY_NAME=qticc
PYTHON_VERSION?=3.12
DOCKER_IMAGE=qticc-test-$(PYTHON_VERSION)

.PHONY: all build test clean clippy fmt docker-build docker-test-x11 docker-test-wl

all: build

build:
	cargo build --release

test:
	cargo test --lib -- --nocapture

clean:
	cargo clean

clippy:
	cargo clippy -- -D warnings

fmt:
	cargo fmt -- --check

# Docker targets for local development and verification
docker-build:
	docker build -t $(DOCKER_IMAGE) --build-arg PYTHON_VERSION=$(PYTHON_VERSION) -f Dockerfile.test .

docker-test-x11: docker-build
	docker run --rm --privileged $(DOCKER_IMAGE) ./scripts/run-x11-tests $(PYTHON_VERSION)

docker-test-wl: docker-build
	docker run --rm --privileged $(DOCKER_IMAGE) ./scripts/run-wl-tests $(PYTHON_VERSION)

docker-shell: docker-build
	docker run --rm -it --privileged $(DOCKER_IMAGE) bash
