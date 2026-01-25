# Multi-stage build for bip39-encode
FROM rust:1.87-slim-bullseye AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    g++ \
    git \
    make \
    curl \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy Cargo files
COPY Cargo.toml Cargo.lock ./

# Create a dummy src to build dependencies
RUN mkdir -p src && \
    echo "fn main() {}" > src/main.rs && \
    echo "pub struct GrammarChecker;" > src/lib.rs && \
    echo "pub enum Language { English }" >> src/lib.rs && \
    cargo build --release && \
    rm -rf src

# Copy source code
COPY src ./src

# Build the actual application
RUN cargo build --release

# Download and decompress nlprule binary files to persistent location
RUN mkdir -p /opt/nlprule-data && \
    curl -L https://github.com/bminixhofer/nlprule/releases/download/0.6.4/en_tokenizer.bin.gz | \
        gunzip > /opt/nlprule-data/en_tokenizer.bin && \
    curl -L https://github.com/bminixhofer/nlprule/releases/download/0.6.4/en_rules.bin.gz | \
        gunzip > /opt/nlprule-data/en_rules.bin

# Runtime stage
FROM debian:bullseye-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl1.1 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy binaries from builder
COPY --from=builder /app/target/release/bip39-encode /app/bip39-encode
COPY --from=builder /app/target/release/tag_words /app/tag_words

# Copy nlprule binary files
COPY --from=builder /opt/nlprule-data /opt/nlprule-data

# Set working directory
WORKDIR /app

# Run the application (it will look for binaries in /opt/nlprule-data/)
ENTRYPOINT ["/app/bip39-encode"]
