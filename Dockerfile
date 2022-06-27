# Copyright (c) 2022 ZettaScale Technology
#
# This program and the accompanying materials are made available under the
# terms of the Eclipse Public License 2.0 which is available at
# http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
# which is available at https://www.apache.org/licenses/LICENSE-2.0.
#
# SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
#
# Contributors:
#   ZettaScale Zenoh Team, <zenoh@zettascale.tech>

###
### Dockerfile building the Eclipse zenoh router (zenohd)
###
#
# To build this Docker image: 
#   docker build -t eclipse/zenoh .
#
# To run this Docker image (without UDP multicast scouting):
#    docker run --init -p 7447:7447/tcp -p 7447:7447/udp -p 8000:8000/tcp eclipse/zenoh
#
# To run this Docker image with UDP multicast scouting (Linux only):
#    docker run --init -net host eclipse/zenoh


FROM --platform=$BUILDPLATFORM alpine:latest AS builder

RUN apk add --no-cache curl gcc musl-dev llvm-dev clang-dev git

ARG TARGETPLATFORM
RUN echo "Setting variables for ${TARGETPLATFORM:=linux/amd64}" && \
    case "${TARGETPLATFORM}" in \
    linux/amd64) \
    echo "x86_64-unknown-linux-musl" > rust-target; \
    break;; \
    linux/arm64) \
    echo "aarch64-unknown-linux-musl" > rust-target; \
    break;; \
    *) echo "unsupported platform ${TARGETPLATFORM}";; \
    esac

COPY rust-toolchain rust-toolchain
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --default-host `cat rust-target` --default-toolchain `cat rust-toolchain`

ENV PATH /root/.cargo/bin:$PATH

COPY . .

RUN cargo build --release


FROM alpine:latest

RUN apk add --no-cache libgcc libstdc++

COPY --from=builder target/release/zenohd /
COPY --from=builder target/release/*.so /

RUN echo '#!/bin/ash' > /entrypoint.sh
RUN echo 'echo " * Starting: /zenohd $*"' >> /entrypoint.sh
RUN echo 'exec /zenohd $*' >> /entrypoint.sh
RUN chmod +x /entrypoint.sh

EXPOSE 7447/udp
EXPOSE 7447/tcp
EXPOSE 8000/tcp

ENV RUST_LOG info

ENTRYPOINT ["/entrypoint.sh"]
