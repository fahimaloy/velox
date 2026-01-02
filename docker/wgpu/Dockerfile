FROM ubuntu:24.04

# Install basics
RUN apt-get update \
    && apt-get install -y software-properties-common \
    && add-apt-repository universe \
    && apt-get update \
    && apt-get install -y build-essential curl ca-certificates libvulkan1 vulkan-tools mesa-vulkan-drivers libwayland-dev libx11-dev libxkbcommon-dev libgl1-mesa-dev git \
    && rm -rf /var/lib/apt/lists/*

# Install Rust (via rustup)
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

# Download SwiftShader
ARG SWIFT_URL="https://github.com/google/SwiftShader/releases/latest/download/SwiftShader-linux-x64.tar.gz"
RUN mkdir -p /opt/swiftshader
ARG BUILD_SWIFT_FROM_SOURCE="true"
# Attempt to download the latest SwiftShader release asset; if unavailable and
# BUILD_SWIFT_FROM_SOURCE is true, clone the upstream repository and build it
# using CMake + Ninja. This keeps the Docker image reproducible while allowing
# a source fallback when prebuilt assets are not published.
RUN set -eux; \
    if curl -fsSL "$SWIFT_URL" -o /tmp/swift.tar.gz; then \
        tar -xzf /tmp/swift.tar.gz -C /opt/swiftshader; \
        if [ -f /opt/swiftshader/etc/vulkan/icd.d/10_swiftshader_icd.json ]; then \
            mkdir -p /etc/vulkan/icd.d; \
            cp /opt/swiftshader/etc/vulkan/icd.d/10_swiftshader_icd.json /etc/vulkan/icd.d/10_swiftshader_icd.json; \
        fi; \
    else \
        echo "SwiftShader download failed; attempting to build from source"; \
        if [ "${BUILD_SWIFT_FROM_SOURCE}" = "true" ]; then \
            apt-get update; \
            apt-get install -y cmake ninja-build python3 pkg-config libx11-dev libxrandr-dev libxcb1-dev libxkbcommon-dev libvulkan-dev || true; \
            # Try primary upstream mirror, fall back to GitHub mirror if needed
            if git clone --depth 1 https://swiftshader.googlesource.com/SwiftShader /tmp/SwiftShader 2>/dev/null; then true; else git clone --depth 1 https://github.com/google/swiftshader.git /tmp/SwiftShader; fi; \
            (cd /tmp/SwiftShader && git submodule update --init --recursive || true); \
            mkdir -p /tmp/SwiftShader/build; \
            (cd /tmp/SwiftShader/build && cmake .. -G Ninja -DCMAKE_BUILD_TYPE=Release || cmake .. -G "Unix Makefiles" -DCMAKE_BUILD_TYPE=Release); \
            (cd /tmp/SwiftShader/build && cmake --build . --parallel || true); \
            # Copy any found ICD json and libraries into /opt/swiftshader
            mkdir -p /opt/swiftshader/lib /opt/swiftshader/etc/vulkan/icd.d; \
            find /tmp/SwiftShader -type f -name '*swiftshader*icd*.json' -exec cp {} /opt/swiftshader/etc/vulkan/icd.d/10_swiftshader_icd.json \; || true; \
            find /tmp/SwiftShader -type f -name 'libvk_swiftshader.*' -exec cp {} /opt/swiftshader/lib/ \; || true; \
        else \
            echo "BUILD_SWIFT_FROM_SOURCE is false; continuing without SwiftShader"; \
        fi; \
    fi

ENV LD_LIBRARY_PATH=/opt/swiftshader/lib:${LD_LIBRARY_PATH:-}
ENV VK_ICD_FILENAMES=/opt/swiftshader/etc/vulkan/icd.d/10_swiftshader_icd.json

WORKDIR /work
COPY . /work

RUN rustup default stable
RUN cargo build -p velox-renderer --features wgpu

CMD ["bash"]
