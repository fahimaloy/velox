FROM ubuntu:24.04

# Install basics
RUN apt-get update \
    && apt-get install -y software-properties-common \
    && add-apt-repository universe \
    && apt-get update \
    && apt-get install -y build-essential curl ca-certificates libvulkan1 vulkan-tools mesa-vulkan-drivers libwayland-dev libx11-dev libxkbcommon-dev git \
    && rm -rf /var/lib/apt/lists/*

# Install Rust (via rustup)
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

# Download SwiftShader
RUN mkdir -p /opt/swiftshader
RUN curl -fsSL "https://github.com/google/SwiftShader/releases/download/2024-12-01/SwiftShader-linux-x64-2024-12-01.tar.gz" -o /tmp/swift.tar.gz \
    && tar -xzf /tmp/swift.tar.gz -C /opt/swiftshader

ENV LD_LIBRARY_PATH=/opt/swiftshader/lib:$LD_LIBRARY_PATH
ENV VK_ICD_FILENAMES=/opt/swiftshader/etc/vulkan/icd.d/10_swiftshader_icd.json

WORKDIR /work
COPY . /work

RUN rustup default stable
RUN cargo build -p velox-renderer --features wgpu

CMD ["bash"]
