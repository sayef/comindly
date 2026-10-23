# comindly in a container: the web UI on port 7878, everything it keeps under /data.
# `release` (default) compiles inside Docker; `local` packages prebuilt Linux binaries.
FROM node:24-bookworm-slim AS ui
WORKDIR /src/ui
COPY ui/package.json ui/package-lock.json ./
RUN npm ci
COPY ui/ ./
RUN npm run build

FROM rust:1.97-bookworm AS build
RUN apt-get update && apt-get install -y --no-install-recommends protobuf-compiler libprotobuf-dev && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY . .
COPY --from=ui /src/ui/dist ui/dist
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked && cp target/release/comindly /usr/local/bin/comindly

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates git openssh-client \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --uid 10001 --home-dir /data --create-home comindly
USER comindly
WORKDIR /data
ENV HOME=/data
# One JSON event per line, for platform logs (Datadog, CloudWatch, Loki).
ENV COMINDLY_LOG_FORMAT=json
VOLUME /data
EXPOSE 7878
ENTRYPOINT ["comindly"]
CMD ["server"]

# A binary built outside Docker for each platform: scripts/docker-local.sh, or the release workflow.
FROM runtime AS local
ARG TARGETARCH
COPY --chmod=755 dist/linux/${TARGETARCH}/comindly /usr/local/bin/comindly

FROM runtime AS release
COPY --from=build /usr/local/bin/comindly /usr/local/bin/comindly
