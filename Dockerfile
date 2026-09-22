FROM node:20-alpine AS frontend
WORKDIR /app/glaudb-admin
COPY glaudb-admin/package*.json ./
RUN npm install
COPY glaudb-admin ./
RUN npm run build

FROM rust:1.80-slim-bookworm AS builder
WORKDIR /app
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY --from=frontend /app/glaudb-admin/dist ./glaudb-admin/dist
RUN cargo build --release

FROM debian:bookworm-slim
WORKDIR /app
RUN apt-get update && apt-get install -y ca-certificates openssl && rm -rf /var/lib/apt/lists/*
RUN mkdir -p /app/data/uploads /app/data/backups
COPY --from=builder /app/target/release/glaudb /app/glaudb
EXPOSE 8080
EXPOSE 8081
CMD ["/app/glaudb", "serve", "--db", "/app/data/glaudb.db", "--uploads", "/app/data/uploads", "--backups", "/app/data/backups"]
