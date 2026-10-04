# syntax=docker/dockerfile:1

FROM rust:1-bookworm AS base

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential pkg-config clang ninja-build python3 ca-certificates \
    libgl1-mesa-dev libegl1-mesa-dev libx11-dev libxrandr-dev libxi-dev \
    libxcursor-dev libxkbcommon-dev libwayland-dev libvulkan-dev \
  && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Pre-cache deps (improve build speed)
COPY Cargo.toml Cargo.lock ./
COPY velox-core/Cargo.toml velox-core/Cargo.toml
COPY velox-dom/Cargo.toml velox-dom/Cargo.toml
COPY velox-sfc/Cargo.toml velox-sfc/Cargo.toml
COPY velox-style/Cargo.toml velox-style/Cargo.toml
COPY velox-renderer/Cargo.toml velox-renderer/Cargo.toml
COPY veloxc/Cargo.toml veloxc/Cargo.toml
# Every workspace member listed in the root manifest needs its own manifest
# here, including the examples — otherwise `cargo fetch` stops at the first
# member it cannot load.
COPY examples/counter/Cargo.toml examples/counter/Cargo.toml
COPY examples/todo/Cargo.toml examples/todo/Cargo.toml
COPY examples/showcase/Cargo.toml examples/showcase/Cargo.toml

# A member whose manifest declares a target whose source file is absent fails
# with "no targets specified in the manifest". The empty placeholders below
# satisfy that check for dependency resolution only; the real sources land in
# this image with `COPY . .` in the builder and test stages.
RUN for crate in velox-core velox-dom velox-sfc velox-style velox-renderer; do \
      mkdir -p "$crate/src" && touch "$crate/src/lib.rs"; \
    done \
 && mkdir -p veloxc/src/bin && touch veloxc/src/lib.rs veloxc/src/bin/main.rs \
 && for example in counter todo showcase; do \
      mkdir -p "examples/$example/src" && touch "examples/$example/src/main.rs"; \
    done \
 && mkdir -p velox-core/benches velox-renderer/benches \
 && touch velox-core/benches/signal_read.rs velox-renderer/benches/skia_render_bench.rs

RUN cargo fetch

FROM base AS builder
COPY . .
RUN cargo build --workspace

FROM base AS test
COPY . .
# Don't enable all features in CI tests — `skia-native` pulls large C++ deps
# that often fail in CI. Run workspace tests without optional native features.
RUN cargo test --workspace --no-fail-fast

