binary_name := "qticc"
python_version := env("PYTHON_VERSION", "3.12")
qtile_repo := env("QTILE_REPO", "https://github.com/qtile/qtile")
qtile_branch := env("QTILE_BRANCH", "master")
docker_image := "qticc-test-" + python_version

default: build

build:
    cargo build --release

test:
    cargo test --lib -- --nocapture

clippy:
    cargo clippy --all-features -- -D warnings

_docker-build:
    docker buildx build \
        -t {{docker_image}} \
        --build-arg PYTHON_VERSION={{python_version}} \
        --build-arg QTILE_REPO={{qtile_repo}} \
        --build-arg QTILE_BRANCH={{qtile_branch}} \
        -f Dockerfile.test .

docker-test-x11: _docker-build
    docker run --rm --privileged {{docker_image}} ./.github/scripts/run-x11-tests {{python_version}}

docker-test-wl: _docker-build
    docker run --rm --privileged {{docker_image}} ./.github/scripts/run-wl-tests {{python_version}}

docker-coverage: _docker-build
    mkdir -p target/tarpaulin
    docker run --rm --privileged -v $PWD/target:/app/target {{docker_image}} ./.github/scripts/run-coverage

docker-shell: _docker-build
    docker run --rm -it --privileged {{docker_image}} bash

clean:
    cargo clean
