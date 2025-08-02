FROM ubuntu:22.04

# Set environment variables to prevent interactive prompts
ENV DEBIAN_FRONTEND=noninteractive
ENV TZ=UTC

# Update and install essential packages
RUN apt-get update && apt-get install -y \
    curl \
    wget \
    git \
    build-essential \
    sudo \
    python3 \
    python3-pip \
    vim \
    nano \
    # Additional packages for vector database development
    pkg-config \
    libssl-dev \
    # For potential C++ extensions
    cmake \
    g++ \
    # For database operations
    sqlite3 \
    libsqlite3-dev \
    && rm -rf /var/lib/apt/lists/*

# Install Node.js 20.x (LTS) from NodeSource
RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - && \
    apt-get install -y nodejs && \
    rm -rf /var/lib/apt/lists/*

# Install Rust (for potential vector database optimisations)
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

# Install global npm packages for TypeScript development and AI/Vector tools
RUN npm install -g \
    typescript \
    ts-node \
    @types/node \
    npm@latest \
    # MCP server development tools
    @modelcontextprotocol/sdk \
    # Build tools
    turbo \
    pnpm

# Create developer user with sudo privileges
RUN useradd -m -s /bin/bash developer && \
    echo "developer:developer" | chpasswd && \
    usermod -aG sudo developer && \
    echo "developer ALL=(ALL) NOPASSWD:ALL" >> /etc/sudoers

# Switch to developer user
USER developer
WORKDIR /home/developer

# Set up Rust for developer user
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y && \
    echo 'source $HOME/.cargo/env' >> /home/developer/.bashrc

# Create project directory
RUN mkdir -p /home/developer/fabstir-ai-vector-db

# Set up npm global directory for the developer user
RUN mkdir -p /home/developer/.npm-global && \
    npm config set prefix '/home/developer/.npm-global' && \
    echo 'export PATH=/home/developer/.npm-global/bin:$PATH' >> /home/developer/.bashrc

# Create directories for vector data persistence
RUN mkdir -p /home/developer/fabstir-ai-vector-db/data/vectors && \
    mkdir -p /home/developer/fabstir-ai-vector-db/data/indices && \
    mkdir -p /home/developer/fabstir-ai-vector-db/logs

# Expose ports
# 7530 for Vector Database API
# 7531 for MCP Server
# 7532 for Admin/Debug interface
EXPOSE 7530 7531 7532

# Set the working directory
WORKDIR /home/developer/fabstir-ai-vector-db

# Keep container running
CMD ["/bin/bash"]