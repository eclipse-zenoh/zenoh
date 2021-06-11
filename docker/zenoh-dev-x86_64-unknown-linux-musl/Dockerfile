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
### Dockerfile for an Alpine 64 bits container with all required tools to build Eclipse zenoh
###

# List of installed tools:
#  * for zenoh:
#    - Rust with stable toolchain
#    - curl gcc musl-dev
#  * for zbackend_influxdb:
#    - openssl-dev
#  * for zbackend-fs (because of rocksdb dependency)
#    - llvm9-dev clang-dev g++ linux-headers

FROM alpine:latest

ARG target=x86_64-unknown-linux-musl

RUN apk add --no-cache curl gcc musl-dev openssl-dev llvm9-dev clang-dev g++ linux-headers

ENV PATH /root/.cargo/bin:$PATH
# Required to build cdylib crates (plugins and backends libs)
ENV RUSTFLAGS '-Ctarget-feature=-crt-static'
ENV CARGO_BUILD_TARGET_DIR=target/${target}

RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --default-host ${target} --default-toolchain stable
