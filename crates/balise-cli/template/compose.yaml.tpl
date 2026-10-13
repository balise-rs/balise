# The dev environment, run by `balise dev`: in the container, the supervisor rebuilds and
# restarts the app at each change. `balise dev` reads the port and the shutdown timeout
# from balise.toml and gives them to this file: run it rather than `docker compose`.
# Not for production.
services:
  app:
    build:
      context: .
      target: dev
{{#if published}}      args:
        BALISE_VERSION: ${BALISE_VERSION:?run `balise dev`}
{{/if}}    init: true
    user: ${BALISE_DEV_USER:?run `balise dev`}
    ports:
      - 127.0.0.1:${BALISE_DEV_PORT:?run `balise dev`}:${BALISE_DEV_PORT}
    environment:
      - CLICOLOR_FORCE
    # Longer than `shutdown.timeout`, so that the app stops cleanly.
    stop_grace_period: ${BALISE_DEV_STOP_GRACE:?run `balise dev`}
    volumes:
      - .:/app
      # Kept between runs: without them, every start compiles everything again.
      - target:/app/target
      - registry:/usr/local/cargo/registry
      - git:/usr/local/cargo/git
{{#if local}}      # The local checkout of Balise (`--balise-path`), and its CLI built from it.
      - {{balise_path}}:{{balise_path}}:ro
    command:
      - sh
      - -c
      - >-
        export PATH="/app/target/balise-cli/bin:$$PATH"
        && (cd / && cargo install --locked --path {{balise_path}}/crates/balise-cli
        --root /app/target/balise-cli --target-dir /app/target/balise-cli)
        && exec balise-cli dev --inside
{{/if}}
volumes:
  target:
  registry:
  git:
