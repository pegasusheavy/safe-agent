# Stage 1: Build the safe-agent binary
FROM rust:1.88-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
COPY config.example.toml ./

# Cache buster — pass --build-arg CACHEBUST=$(date +%s) to force rebuild
ARG CACHEBUST=1
RUN cargo build --release

# Stage 2: Runtime with Node.js + Claude CLI + Python
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    git \
    python3 \
    python3-pip \
    python3-venv \
    && rm -rf /var/lib/apt/lists/*

# Install Node.js (LTS) for Claude CLI
RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && rm -rf /var/lib/apt/lists/*

# Install Claude Code CLI globally
RUN npm install -g @anthropic-ai/claude-code

# Install ngrok for tunnel support
RUN curl -fsSL https://bin.equinox.io/c/bNyj1mQVY4c/ngrok-v3-stable-linux-amd64.tgz \
    -o /tmp/ngrok.tgz \
    && tar -xzf /tmp/ngrok.tgz -C /usr/local/bin \
    && rm /tmp/ngrok.tgz \
    && chmod +x /usr/local/bin/ngrok

# Install common Python packages that skills are likely to need
RUN pip3 install --no-cache-dir --break-system-packages \
    requests \
    google-api-python-client \
    google-auth-httplib2 \
    google-auth-oauthlib \
    python-dotenv \
    schedule \
    httpx \
    beautifulsoup4 \
    feedparser \
    icalendar

# Copy safe-agent binary
COPY --from=builder /build/target/release/safe-agent /usr/local/bin/safe-agent

# Data and config directories (mounted at runtime)
RUN mkdir -p /data/safe-agent/skills /config/safe-agent

ENV XDG_DATA_HOME=/data
ENV XDG_CONFIG_HOME=/config

EXPOSE 3031 443

VOLUME ["/data/safe-agent", "/config/safe-agent"]

ENTRYPOINT ["safe-agent"]
