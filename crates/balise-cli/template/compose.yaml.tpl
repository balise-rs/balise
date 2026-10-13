# The dev environment, run by `balise dev`: in the container, the supervisor rebuilds and
# restarts the app at each change. `balise dev` reads the port and the shutdown timeout
# from balise.toml and writes them in .env, which compose reads: `docker compose` works by
# hand too, with the values of the last `balise` command. Not for production.
services:
  app:
    build:
      context: .
      target: dev
{{#if published}}      args:
        BALISE_VERSION: ${BALISE_VERSION:?run `balise dev` once, which writes it in .env}
{{/if}}    init: true
    user: ${BALISE_DEV_USER:?run `balise dev` once, which writes it in .env}
    ports:
      - 127.0.0.1:${BALISE_DEV_PORT:?run `balise dev` once, which writes it in .env}:${BALISE_DEV_PORT}
    environment:
      - CLICOLOR_FORCE
    # Longer than `shutdown.timeout`, so that the app stops cleanly.
    stop_grace_period: ${BALISE_DEV_STOP_GRACE:?run `balise dev` once, which writes it in .env}
    volumes:
      - .:/app
      # Kept between runs: without them, every start compiles everything again.
      - target:/app/target
      - registry:/usr/local/cargo/registry
      - git:/usr/local/cargo/git
{{#if local}}      # The local checkout of Balise (`--balise-path`), which the CLI is built from.
      - {{balise_path}}:{{balise_path}}:ro
{{/if}}
volumes:
  target:
  registry:
  git:
