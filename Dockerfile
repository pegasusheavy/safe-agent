# Stage 1: Build with CUDA SDK
FROM nvidia/cuda:12.8.1-devel-ubuntu24.04 AS builder

# Install Rust
RUN apt-get update && apt-get install -y --no-install-recommends \
    curl \
    build-essential \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

WORKDIR /build

# Copy everything needed for the build
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
COPY config.example.toml ./

RUN cargo build --release

# Stage 2: Runtime with CUDA libraries (includes nvrtc)
FROM nvidia/cuda:12.8.1-runtime-ubuntu24.04

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    patch \
    libnvrtc12 \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -s /bin/bash agent

COPY --from=builder /build/target/release/safe-agent /usr/local/bin/safe-agent
COPY config.example.toml /etc/safe-agent/config.example.toml

# Data directory
RUN mkdir -p /data/safe-agent && chown agent:agent /data/safe-agent

USER agent

ENV XDG_DATA_HOME=/data
ENV XDG_CONFIG_HOME=/config
ENV NVIDIA_VISIBLE_DEVICES=all
ENV NVIDIA_DRIVER_CAPABILITIES=compute,utility

EXPOSE 3030

VOLUME ["/data/safe-agent", "/config"]

ENTRYPOINT ["safe-agent"]
