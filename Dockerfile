# Stage 1: Build the safe-agent binary (musl/static for Alpine)
FROM rust:1.88-alpine AS builder

RUN apk add --no-cache musl-dev pkgconf perl make openssl-dev openssl-libs-static

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
COPY config/ config/
COPY config.example.toml ./

# Cache buster — pass --build-arg CACHEBUST=$(date +%s) to force rebuild
ARG CACHEBUST=1
RUN cargo build --release

# Stage 2: Runtime (Alpine)
FROM alpine:3.21

# System packages — Alpine 3.21 ships Node 22.x LTS and Python 3.12
# coreutils provides chroot for the jail entrypoint
RUN apk add --no-cache \
    ca-certificates curl git bash su-exec coreutils \
    nodejs npm python3 py3-pip

# Install Claude Code CLI globally
RUN npm install -g @anthropic-ai/claude-code

# Install ngrok for tunnel support.
# The equinox.io tgz endpoint is defunct; pull the .deb from the ngrok S3
# apt repo and extract the static binary (works fine on Alpine/musl).
RUN apk add --no-cache --virtual .ngrok-deps binutils xz && \
    ARCH="$(uname -m)" && \
    if [ "$ARCH" = "x86_64" ]; then NGROK_ARCH="amd64"; \
    elif [ "$ARCH" = "aarch64" ]; then NGROK_ARCH="arm64"; \
    else NGROK_ARCH="amd64"; fi && \
    curl -fsSL "https://ngrok-agent.s3.amazonaws.com/pool/main/n/ngrok/ngrok_3.36.1-0_${NGROK_ARCH}.deb" \
    -o /tmp/ngrok.deb && \
    cd /tmp && ar x ngrok.deb && tar -xf data.tar.xz && \
    mv /tmp/usr/local/bin/ngrok /usr/local/bin/ngrok && \
    chmod +x /usr/local/bin/ngrok && \
    rm -rf /tmp/ngrok.deb /tmp/data.tar.xz /tmp/control.tar.* /tmp/debian-binary /tmp/usr && \
    apk del .ngrok-deps

# Install common Python packages that skills are likely to need
RUN pip install --no-cache-dir --break-system-packages \
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

# Chroot jail entrypoint script
COPY scripts/chroot-jail.sh /usr/local/bin/chroot-jail.sh
RUN chmod +x /usr/local/bin/chroot-jail.sh

# Non-root user for running safe-agent (jail setup still runs as root).
# UID/GID default to 1000 to match typical host users — override with
# --build-arg to match your host user's uid/gid for bind-mount perms.
ARG SAFE_UID=1000
ARG SAFE_GID=1000
RUN addgroup -g "${SAFE_GID}" -S safeagent && \
    adduser -u "${SAFE_UID}" -G safeagent -h /home/safeagent -s /bin/bash -S safeagent

# Pre-create the jail root and volume mount points
RUN mkdir -p /jail /data/safe-agent/skills /config/safe-agent /home/safeagent && \
    chown -R safeagent:safeagent /data/safe-agent /config/safe-agent /home/safeagent

ENV XDG_DATA_HOME=/data
ENV XDG_CONFIG_HOME=/config
ENV HOME=/home/safeagent

EXPOSE 3031 443

VOLUME ["/data/safe-agent", "/config/safe-agent"]

# The entrypoint script builds the chroot jail at startup, then
# chroots into it and exec's safe-agent.  Pass NO_JAIL=1 to bypass.
ENTRYPOINT ["chroot-jail.sh"]
