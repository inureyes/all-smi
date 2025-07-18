# Multi-stage build for optimal image size
FROM rust:1.88-slim as builder

# Install system dependencies for cross-compilation
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Set the working directory
WORKDIR /app

# Copy the Cargo.toml and Cargo.lock files
COPY Cargo.toml Cargo.lock ./

# Copy the source code
COPY src/ ./src/

# Build the application in release mode
RUN cargo build --release --bin all-smi

# Runtime stage with minimal image
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create a non-root user
RUN useradd -r -s /bin/false allsmi

# Copy the binary from the builder stage
COPY --from=builder /app/target/release/all-smi /usr/local/bin/all-smi

# Make the binary executable
RUN chmod +x /usr/local/bin/all-smi

# Switch to non-root user
USER allsmi

# Expose the default API port
EXPOSE 9090

# Set the default command
ENTRYPOINT ["/usr/local/bin/all-smi"]
CMD ["api", "--port", "9090"]