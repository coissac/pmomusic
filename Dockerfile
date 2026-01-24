# ===================================
# Stage 1: Build Vue.js webapp
# ===================================
FROM node:22-alpine AS webapp-builder

WORKDIR /webapp

# Copy webapp package files
COPY pmoapp/webapp/package*.json ./

# Install dependencies
RUN npm ci --production=false

# Copy webapp source
COPY pmoapp/webapp/ ./

# Build the webapp
RUN npm run build

# ===================================
# Stage 2: Build Rust binary
# ===================================
FROM rustlang/rust:nightly-bookworm AS rust-builder

WORKDIR /build

# Install system dependencies for building
RUN apt-get update && apt-get install -y \
    libsoxr-dev \
    libasound2-dev \
    pkg-config \
    cmake \
    && rm -rf /var/lib/apt/lists/*

# Copy Cargo workspace files
COPY Cargo.toml Cargo.lock ./

# Copy all crates
COPY PMOMusic/ ./PMOMusic/
COPY pmoupnp/ ./pmoupnp/
COPY pmomediarenderer/ ./pmomediarenderer/
COPY pmomediaserver/ ./pmomediaserver/
COPY pmoconfig/ ./pmoconfig/
COPY pmoutils/ ./pmoutils/
COPY pmodidl/ ./pmodidl/
COPY pmoserver/ ./pmoserver/
COPY pmocache/ ./pmocache/
COPY pmocovers/ ./pmocovers/
COPY pmoaudiocache/ ./pmoaudiocache/
COPY pmoaudio/ ./pmoaudio/
COPY pmoqobuz/ ./pmoqobuz/
COPY pmoparadise/ ./pmoparadise/
COPY pmoradiofrance/ ./pmoradiofrance/
COPY pmosource/ ./pmosource/
COPY pmoplaylist/ ./pmoplaylist/
COPY pmoflac/ ./pmoflac/
COPY pmometadata/ ./pmometadata/
COPY pmocontrol/ ./pmocontrol/
COPY pmoaudio-ext/ ./pmoaudio-ext/
COPY pmoapp/ ./pmoapp/

# Copy the webapp dist from previous stage
COPY --from=webapp-builder /webapp/dist ./pmoapp/webapp/dist/

# Build the Rust binary in release mode
RUN cargo build --release --bin PMOMusic

# Strip debug symbols to reduce binary size
RUN strip /build/target/release/PMOMusic

# ===================================
# Stage 3: Minimal runtime image
# ===================================
FROM debian:bookworm-slim

ARG BUILD_DATE
LABEL org.opencontainers.image.created=$BUILD_DATE
LABEL org.opencontainers.image.title="PMOMusic UPNP Music Server"
LABEL org.opencontainers.image.version="0.1"
LABEL org.opencontainers.image.authors="eric@coissac.eu"
LABEL org.opencontainers.image.licenses="CeCILL-2.0"
LABEL org.opencontainers.image.url="https://gargoton.petite-maison-orange.fr/eric/pmomusic"
LABEL org.opencontainers.image.description="PMOMusic est une application Rust proposant en un binaire unique une Serveur de Musique, et une point de controle UPNP controlable via une application web."

# Install only runtime dependencies
RUN apt-get update && apt-get install -y \
    libsoxr0 \
    libasound2 \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create a non-root user
RUN useradd -m -u 1000 pmomusic

# Copy the binary from builder
COPY --from=rust-builder /build/target/release/PMOMusic /usr/local/bin/PMOMusic

# Set ownership
RUN chown pmomusic:pmomusic /usr/local/bin/PMOMusic

# Switch to non-root user
USER pmomusic

# Create directories for configuration and cache
RUN mkdir -p /home/pmomusic/.pmomusic

# Set working directory
WORKDIR /home/pmomusic

# Expose default port (adjust if needed)
EXPOSE 8080

# Health check (adjust the URL if needed)
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD ["/usr/local/bin/PMOMusic", "--help"] || exit 1

# Run the binary
ENTRYPOINT ["/usr/local/bin/PMOMusic"]
