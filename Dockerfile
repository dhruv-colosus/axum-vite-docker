# -------------------------------------------------
# Stage 1 – React client (arm64)
# -------------------------------------------------
FROM --platform=linux/arm64 node:20-alpine AS client-builder
WORKDIR /app

#  lock-files first → layer cache
COPY client/pnpm-lock.yaml client/package.json ./client/
RUN corepack enable && cd client && pnpm install --frozen-lockfile

#  actual source
COPY client ./client
RUN cd client && npm run build                      # → client/dist

# -------------------------------------------------
# Stage 2 – Rust backend (arm64 ↦ musl)
# -------------------------------------------------
FROM --platform=linux/arm64 rustlang/rust:nightly-slim AS backend-builder

RUN apt-get update && apt-get install -y musl-tools \
 && rustup target add aarch64-unknown-linux-musl

WORKDIR /app

# ---- 1) dependency-cache layer ----
COPY backend/Cargo.toml backend/Cargo.lock ./backend/
RUN mkdir backend/src && echo 'fn main(){}' > backend/src/main.rs
RUN cargo build --release \
     --target aarch64-unknown-linux-musl \
     --manifest-path backend/Cargo.toml

# ---- 2) real build ----
COPY backend ./backend
RUN cargo build --release \
     --target aarch64-unknown-linux-musl \
     --manifest-path backend/Cargo.toml

# -------------------------------------------------
# Stage 3 – lean runtime (arm64, static)
# -------------------------------------------------
FROM --platform=linux/arm64 gcr.io/distroless/cc
WORKDIR /app

#  static binary → tiny distroless image
COPY --from=backend-builder \
     /app/target/aarch64-unknown-linux-musl/release/backend \
     ./server

#  React build
COPY --from=client-builder /app/client/dist ./static

EXPOSE 8001
CMD ["./server"]
