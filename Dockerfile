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
RUN curl -sSL https://ngrok-agent.s3.amazonaws.com/ngrok-v3-stable-linux-amd64.tgz \
    | tar -xz -C /usr/local/bin

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

EXPOSE 3031

VOLUME ["/data/safe-agent", "/config/safe-agent"]

ENTRYPOINT ["safe-agent"]
