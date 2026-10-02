# Tuff Console image, published as ghcr.io/kannandreams/tuff-console.
#
# The image packages the release binaries instead of compiling them, so it
# runs the same `tuff` that the GitHub release ships. The build context is a
# folder holding one binary per platform:
#
#   <context>/amd64/tuff   from tuff-x86_64-unknown-linux-gnu.tar.gz
#   <context>/arm64/tuff   from tuff-aarch64-unknown-linux-gnu.tar.gz
#
#   docker buildx build --platform linux/amd64,linux/arm64 -f docker/Dockerfile <context>
#
# The release workflow builds and publishes it on every tag. The binaries are
# built on Ubuntu 24.04 runners, so the base image matches their glibc.
FROM ubuntu:24.04

ARG TARGETARCH

# ca-certificates: the console fetches GitHub's OIDC signing keys over HTTPS.
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --home-dir /data --shell /usr/sbin/nologin tuff \
 && mkdir /data \
 && chown tuff:tuff /data

COPY --chmod=0755 ${TARGETARCH}/tuff /usr/local/bin/tuff

USER tuff
VOLUME /data
EXPOSE 7474
ENTRYPOINT ["tuff", "console", "serve", "--addr", "0.0.0.0:7474", "--public-read", "--data", "/data"]
