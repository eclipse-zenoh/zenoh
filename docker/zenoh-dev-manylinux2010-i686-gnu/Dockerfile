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
### Dockerfile for a Linux x86 32 bits container with all required tools to build Eclipse zenoh and its APIs
###

# List of installed tools:
#  * for zenoh:
#    - Rust with stable toolchain
#    - dpkg + cargo-deb for Debian packages
#    - rpm-build + cargo-rpm for RedHat packages
#  * for zbackend_influxdb:
#    - openssl-devel
#  * for zbackend-fs (because of rocksdb dependency)
#    - clang-devel llvm-devel
#  * for zenoh-python
#    - maturin
#  * for zenoh-c:
#    - cbindgen
#  * for zenoh-pico:
#    - cmake3

FROM quay.io/pypa/manylinux2010_i686:latest

ARG target=i686-unknown-linux-gnu

# EPEL required for some extra packages
RUN yum install -y epel-release
RUN yum install -y dpkg rpm-build openssl-devel clang-devel llvm-devel cmake3
RUN mv /usr/bin/cmake /usr/bin/cmake2; ln -s /usr/bin/cmake3 /usr/bin/cmake

ENV PATH /root/.cargo/bin:$PATH
# Use Python3.9
ENV PATH /opt/python/cp39-cp39/bin/:$PATH
# Otherwise `cargo new` errors
ENV USER root
ENV CARGO_BUILD_TARGET_DIR=target/${target}

RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --default-host ${target} --default-toolchain stable \
    && pip install maturin==0.9.0 \
    && cargo install cbindgen cargo-deb cargo-rpm
