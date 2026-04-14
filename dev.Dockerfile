FROM rust:1.87-slim-bullseye

# Install all build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    g++ \
    git \
    make \
    protobuf-compiler \
    libglib2.0-dev \
    libgtk-3-dev \
    libwebkit2gtk-4.0-dev \
    libayatana-appindicator3-dev \
    librsvg2-dev \
    && rm -rf /var/lib/apt/lists/*

# Install cargo-watch for hot reloading
RUN cargo install cargo-watch --locked --version 8.4.0

WORKDIR /app

# Set environment variable for incremental compilation
ENV RUST_INCREMENTAL=1
