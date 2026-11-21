FROM mirror.gcr.io/library/rust:1.94-slim-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
# Cargo needs every workspace member's manifest; only the relay is built.
COPY crates crates
RUN cargo build --release --locked --package last-harvest-relay \
    && cp target/release/last-harvest-relay /last-harvest-relay

FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=build /last-harvest-relay /usr/local/bin/last-harvest-relay
ENV LAST_HARVEST_RELAY_PORT=8080
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 CMD ["/usr/local/bin/last-harvest-relay", "--healthcheck"]
ENTRYPOINT ["/usr/local/bin/last-harvest-relay"]
