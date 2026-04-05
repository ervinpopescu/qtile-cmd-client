# Makefile for qtile-cmd-client (qticc)

BINARY_NAME=qticc
PYTHON_VERSION?=3.12
QTILE_REPO?=https://github.com/qtile/qtile
QTILE_BRANCH?=master
DOCKER_IMAGE=qticc-test-$(PYTHON_VERSION)

.PHONY: all build test clean clippy fmt docker-build docker-test-x11 docker-test-wl docker-coverage docker-shell

all: build

build:
	cargo build --release

test:
	cargo test --lib -- --nocapture

clippy:
	cargo clippy -- -D warnings

# Docker targets for local development and verification
.dockerbuilt: Dockerfile.test deps .github/scripts/install-deps .github/scripts/install-qtile
	docker buildx build -t $(DOCKER_IMAGE) --build-arg PYTHON_VERSION=$(PYTHON_VERSION) --build-arg QTILE_REPO=$(QTILE_REPO) --build-arg QTILE_BRANCH=$(QTILE_BRANCH) -f Dockerfile.test .
	touch .dockerbuilt

docker-build: .dockerbuilt

docker-test-x11: docker-build
	docker run --rm --privileged $(DOCKER_IMAGE) ./.github/scripts/run-x11-tests $(PYTHON_VERSION)

docker-test-wl: docker-build
	docker run --rm --privileged $(DOCKER_IMAGE) ./.github/scripts/run-wl-tests $(PYTHON_VERSION)

docker-coverage: docker-build
	mkdir -p target/tarpaulin
	docker run --rm --privileged -v $(PWD)/target:/app/target $(DOCKER_IMAGE) ./.github/scripts/run-coverage

docker-shell: docker-build
	docker run --rm -it --privileged $(DOCKER_IMAGE) bash

clean:
	cargo clean
	rm -f .dockerbuilt
