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
COPY velox-cli/Cargo.toml velox-cli/Cargo.toml
COPY examples/gallery/Cargo.toml examples/gallery/Cargo.toml
COPY examples/todo/Cargo.toml examples/todo/Cargo.toml

RUN cargo fetch

FROM base AS builder
COPY . .
RUN cargo build --workspace

FROM base AS test
COPY . .
# Don't enable all features in CI tests — `skia-native` pulls large C++ deps
# that often fail in CI. Run workspace tests without optional native features.
RUN cargo test --workspace --no-fail-fast

