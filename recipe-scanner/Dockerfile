FROM debian:bookworm-slim

# Install essential tools for monitoring and security scanning
# Also install X11 libraries needed by Goose CLI
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    bash \
    coreutils \
    iproute2 \
    net-tools \
    procps \
    tcpdump \
    strace \
    inotify-tools \
    clamav \
    clamav-freshclam \
    jq \
    ripgrep \
    sudo \
    python3 \
    bzip2 \
    tar \
    gnupg \
    git \
    libxcb1 \
    libxcb-render0 \
    libxcb-shape0 \
    libxcb-xfixes0 \
    libxkbcommon0 \
    libgl1-mesa-glx \
    libgomp1 \
    && rm -rf /var/lib/apt/lists/*

# Install Node.js (LTS) and npm/npx via NodeSource
RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - && \
    apt-get update && apt-get install -y --no-install-recommends nodejs && \
    npm --version && node --version && npx --version && \
    rm -rf /var/lib/apt/lists/*

# Install Astral uv (provides 'uv' and 'uvx')
RUN curl -LsSf https://astral.sh/uv/install.sh | sh && \
    cp -f /root/.local/bin/uv /usr/local/bin/uv && \
    cp -f /root/.local/bin/uvx /usr/local/bin/uvx && \
    chmod +x /usr/local/bin/uv /usr/local/bin/uvx && \
    uv --version && uvx --version

# Pre-download and install Goose CLI to avoid network issues during runtime
RUN curl -fsSL https://github.com/aaif-goose/goose/releases/download/stable/download_cli.sh | \
    CONFIGURE=false GOOSE_BIN_DIR=/usr/local/bin bash && \
    echo "✅ Goose CLI pre-installed: $(/usr/local/bin/goose --version)"

# Create ClamAV configuration directory and basic config
# Allow non-root 'scanner' to install packages via sudo without password
RUN echo "scanner ALL=(root) NOPASSWD: /usr/bin/apt, /usr/bin/apt-get, /usr/bin/dpkg, /usr/bin/curl, /usr/bin/wget" > /etc/sudoers.d/scanner \
    && chmod 0440 /etc/sudoers.d/scanner \
    && chown root:root /etc/sudoers.d/scanner

RUN mkdir -p /etc/clamav && \
    echo "DatabaseDirectory /var/lib/clamav" > /etc/clamav/freshclam.conf && \
    echo "UpdateLogFile /var/log/clamav/freshclam.log" >> /etc/clamav/freshclam.conf && \
    echo "LogVerbose yes" >> /etc/clamav/freshclam.conf && \
    echo "DatabaseMirror database.clamav.net" >> /etc/clamav/freshclam.conf && \
    mkdir -p /var/log/clamav && \
    chown -R clamav:clamav /var/lib/clamav /var/log/clamav

# Update ClamAV virus definitions
RUN freshclam || true

# Create non-root user and setup directories
RUN useradd -m -u 1000 scanner && \
    mkdir -p /home/scanner/.config/goose && \
    mkdir -p /home/scanner/.local/share/goose && \
    mkdir -p /output && \
    mkdir -p /tmp/goose && \
    mkdir -p /tmp/goose_home && \
    mkdir -p /tmp/training && \
    chown -R scanner:scanner /home/scanner /output /tmp/goose /tmp/goose_home /tmp/training

# Set capabilities on tcpdump to allow non-root network capture
RUN setcap cap_net_raw,cap_net_admin=eip /usr/bin/tcpdump

# Copy Goose configuration
COPY config.yaml /home/scanner/.config/goose/config.yaml

# Copy scanning script, base recipe, and training data decoder
COPY scan-recipe.sh /usr/local/bin/scan-recipe.sh
COPY base_recipe.yaml /docker/base_recipe.yaml
COPY decode-training-data.py /usr/local/bin/decode-training-data.py
RUN chmod +x /usr/local/bin/scan-recipe.sh /usr/local/bin/decode-training-data.py

# Set proper ownership
RUN chown scanner:scanner /home/scanner/.config/goose/config.yaml /docker/base_recipe.yaml

# Switch to non-root user
USER scanner

# Set working directory
WORKDIR /workspace

# Default entrypoint
ENTRYPOINT ["/usr/local/bin/scan-recipe.sh"]
