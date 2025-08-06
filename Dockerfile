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
    # Additional tools for debugging
    netcat \
    jq \
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
    pnpm \
    # Development tools
    nodemon \
    tsx

# Create developer user with UID/GID 1000 to match host user
RUN groupadd -g 1000 developer && \
    useradd -m -u 1000 -g developer -s /bin/bash developer && \
    echo "developer:developer" | chpasswd && \
    usermod -aG sudo developer && \
    echo "developer ALL=(ALL) NOPASSWD:ALL" >> /etc/sudoers

# Create workspace directory (matching fabstir-llm-node setup)
RUN mkdir -p /workspace && \
    chown -R developer:developer /workspace

# Create project directories with proper ownership
RUN mkdir -p /home/developer/fabstir-ai-vector-db/data/vectors && \
    mkdir -p /home/developer/fabstir-ai-vector-db/data/indices && \
    mkdir -p /home/developer/fabstir-ai-vector-db/logs && \
    mkdir -p /home/developer/.npm-global && \
    chown -R developer:developer /home/developer

# Switch to developer user
USER developer
WORKDIR /workspace

# Set up Rust for developer user
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y && \
    echo 'source $HOME/.cargo/env' >> /home/developer/.bashrc

# Set up npm global directory for the developer user
RUN npm config set prefix '/home/developer/.npm-global' && \
    echo 'export PATH=/home/developer/.npm-global/bin:$PATH' >> /home/developer/.bashrc && \
    echo 'export PATH=/home/developer/.cargo/bin:$PATH' >> /home/developer/.bashrc

# Create .bashrc aliases for convenience
RUN echo 'alias ll="ls -la"' >> /home/developer/.bashrc && \
    echo 'alias vector-logs="tail -f /home/developer/fabstir-ai-vector-db/logs/*.log"' >> /home/developer/.bashrc

# Set environment variables for the application
ENV NODE_ENV=development
ENV VECTOR_DB_HOME=/home/developer/fabstir-ai-vector-db
ENV DATA_DIR=/home/developer/fabstir-ai-vector-db/data
ENV LOG_DIR=/home/developer/fabstir-ai-vector-db/logs

# Expose ports
# 7530 for Vector Database API
# 7531 for MCP Server  
# 7532 for Admin/Debug interface
EXPOSE 7530 7531 7532

# Create an entrypoint script
RUN echo '#!/bin/bash\n\
source /home/developer/.bashrc\n\
source /home/developer/.cargo/env\n\
cd /workspace\n\
exec "$@"' > /home/developer/entrypoint.sh && \
    chmod +x /home/developer/entrypoint.sh

# Set the entrypoint
ENTRYPOINT ["/home/developer/entrypoint.sh"]

# Default command
CMD ["/bin/bash"]