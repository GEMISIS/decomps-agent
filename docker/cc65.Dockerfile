# Sandbox image for the Clean Room B (writer / remedy / forge) shells.
# Contains only a C toolchain: no ROM, no nesrom, no network needed at run time.
FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends cc65 make gcc libc6-dev xxd python3 git ca-certificates fceux xvfb xauth procps \
 && rm -rf /var/lib/apt/lists/*
ENV PATH="/usr/games:${PATH}"
WORKDIR /work
