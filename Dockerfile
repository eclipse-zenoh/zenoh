# Copyright (c) 2017, 2020 ADLINK Technology Inc.
#
# This program and the accompanying materials are made available under the
# terms of the Eclipse Public License 2.0 which is available at
# http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
# which is available at https://www.apache.org/licenses/LICENSE-2.0.
#
# SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
#
# Contributors:
#   ADLINK zenoh team, <zenoh@adlink-labs.tech>

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


FROM alpine:latest AS builder

ARG TARGET=x86_64-unknown-linux-musl

RUN apk add --no-cache curl gcc musl-dev llvm-dev clang-dev git

COPY rust-toolchain rust-toolchain
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --default-host ${TARGET} --default-toolchain `cat rust-toolchain`

ENV PATH /root/.cargo/bin:$PATH

# if exists, copy .git directory to be used by git-version crate to determine the version
COPY .gi? .git/

COPY Cargo.toml .
COPY Cargo.lock .
COPY .cargo/config .cargo/config

COPY zenoh zenoh
COPY zenoh-ext zenoh-ext
COPY zenoh-util zenoh-util
COPY backends backends
COPY plugins plugins

RUN cargo build --release --target=${TARGET}


FROM alpine:latest

RUN apk add --no-cache libgcc libstdc++

COPY --from=builder target/x86_64-unknown-linux-musl/release/zenohd /
COPY --from=builder target/x86_64-unknown-linux-musl/release/*.so /

RUN echo '#!/bin/ash' > /entrypoint.sh
RUN echo 'echo " * Starting: /zenohd $*"' >> /entrypoint.sh
RUN echo 'exec /zenohd $*' >> /entrypoint.sh
RUN chmod +x /entrypoint.sh

EXPOSE 7447/udp
EXPOSE 7447/tcp
EXPOSE 8000/tcp

ENV RUST_LOG info

ENTRYPOINT ["/entrypoint.sh"]
