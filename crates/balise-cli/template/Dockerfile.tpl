# The dev stage of this project, built and run by `balise dev`: Rust at the version of
# rust-toolchain.toml, and the Balise CLI{{#if published}} of the project's version{{/if}}.
# The code is not in the image: it is mounted when the container runs. Not for production.
FROM rust:{{rust}}-slim AS dev
{{#if published}}
# The Balise CLI of the project's version, given by `balise dev`.
ARG BALISE_VERSION
RUN CARGO_HOME=/tmp/cargo cargo install balise-cli --version "=${BALISE_VERSION}" --locked --root /usr/local \
    && rm -rf /tmp/cargo
{{/if}}{{#if local}}
# `balise-cli` builds the CLI from the local checkout of Balise (`--balise-path`), mounted
# by compose, then runs it. The build is incremental, in the `target/` volume.
RUN printf '%s\n' '#!/bin/sh' 'set -e' \
    'install="cargo install --quiet --locked --path {{balise_path}}/crates/balise-cli --root /app/target/balise-cli --target-dir /app/target/balise-cli"' \
    '(cd / && { $install --offline 2>/dev/null || $install; })' \
    'exec /app/target/balise-cli/bin/balise-cli "$@"' \
    > /usr/local/bin/balise-cli && chmod +x /usr/local/bin/balise-cli
{{/if}}
WORKDIR /app
# The toolchain of rust-toolchain.toml, when it changed since the image's.
COPY rust-toolchain.toml ./
RUN rustup toolchain install

# Where the volumes go, writable by the user the container runs as.
RUN mkdir -p target "$CARGO_HOME/registry" "$CARGO_HOME/git" \
    && chmod 1777 target "$CARGO_HOME/registry" "$CARGO_HOME/git"

CMD ["balise-cli", "dev", "--inside"]
