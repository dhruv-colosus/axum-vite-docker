# Build stage
FROM rustlang/rust:nightly AS builder

WORKDIR /app

# Copy cargo files for dependency caching
COPY backend/Cargo.toml backend/Cargo.lock ./

# Create a dummy main.rs to build dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm src/main.rs

# Copy the actual source code
COPY backend/src ./src

# Build the application
RUN touch src/main.rs
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install necessary runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the dist directory (client build files) to the root level
COPY backend/dist ./dist
COPY backend/.env ./
# Create backend directory and copy the binary there
RUN mkdir backend
COPY --from=builder /app/target/release/backend ./backend/backend

# Expose the port
EXPOSE 8001

# Set working directory to backend so ../dist resolves correctly
WORKDIR /app/backend

# Run the application
CMD ["./backend"]
