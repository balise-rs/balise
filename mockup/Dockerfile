# Two uses, one file:
# - `balise dev` builds the `dev` stage (see compose.yaml);
# - `balise build` builds the last stage: one static binary with the front end inside.

# Rust and Node, shared by dev and production builds.
FROM rust:1.99-bookworm AS base
COPY --from=node:22-bookworm-slim /usr/local /usr/local
RUN corepack enable
WORKDIR /app

# Dev: the same CLI as on the host, at the project's version, supervising the app and Vite.
FROM base AS dev
RUN cargo install balise-cli --version 0.1.0 --locked
CMD ["balise", "dev", "--inside"]

# Production: dependencies first, so they stay cached while the code changes.
FROM base AS chef
RUN apt-get update && apt-get install -y --no-install-recommends musl-tools \
    && rm -rf /var/lib/apt/lists/*
RUN rustup target add x86_64-unknown-linux-musl && cargo install cargo-chef --locked

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM base AS frontend
COPY frontend/package.json frontend/pnpm-lock.yaml frontend/pnpm-workspace.yaml frontend/
RUN cd frontend && pnpm install --frozen-lockfile
COPY frontend frontend
RUN cd frontend && pnpm build

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --target x86_64-unknown-linux-musl --recipe-path recipe.json
COPY . .
COPY --from=frontend /app/frontend/dist frontend/dist
RUN cargo build --release --target x86_64-unknown-linux-musl

FROM scratch
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/shop /app
ENV BALISE_ENV=prod
EXPOSE 3000
ENTRYPOINT ["/app"]
