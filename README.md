This directory contains some Dockerfiles of the containers that we use to build zenoh and its APIs on various platforms.

# [zenoh-dev-manylinux2010-i686-gnu](https://hub.docker.com/repository/docker/adlinktech/zenoh-dev-manylinux2010-i686-gnu)

A [manylinux2010](https://github.com/pypa/manylinux)-based image to target most of the Linux x86 32-bit platforms.  
It includes:
  * To build zenoh and its backends:
    - Rust with the nightly toolchain by default and `i686-unknown-linux-gnu` as default target
    - openssl-devel (required by InfluxDB backend)
    - clang-devel llvm-devel (required by the file system backend, because of rocksdb dependency)
    - dpkg rpm-build (for debian packaging)
  * To build zenoh-python:
    - All the Python versions provided by manylinux2010
    - maturin
  * To build zenoh-c:
    - cbindgen
  * To build zenoh-pico:
    - cmake3

Usage to build zenoh:
```bash
   docker run --init --rm -v $(pwd):/workdir -w /workdir adlinktech/zenoh-dev-manylinux2010-i686-gnu cargo build --release --bins --lib --examples
```

# [zenoh-dev-manylinux2010-x86_64-gnu](https://hub.docker.com/repository/docker/adlinktech/zenoh-dev-manylinux2010-x86_64-gnu)

A [manylinux2010](https://github.com/pypa/manylinux)-based image to target most of the Linux x86 64-bit platforms.  
It includes:
  * To build zenoh and its backends:
    - Rust with the nightly toolchain by default and `x86_64-unknown-linux-gnu` as default target
    - openssl-devel (required by InfluxDB backend)
    - clang-devel llvm-devel (required by the file system backend, because of rocksdb dependency)
    - dpkg rpm-build (for debian packaging)
  * To build zenoh-python:
    - All the Python versions provided by manylinux2010
    - maturin
  * To build zenoh-c:
    - cbindgen
  * To build zenoh-pico:
    - cmake3

Usage to build zenoh:
```bash
   docker run --init --rm -v $(pwd):/workdir -w /workdir adlinktech/zenoh-dev-manylinux2010-x86_64-gnu cargo build --release --bins --lib --examples
```

# [zenoh-dev-manylinux2014-aarch64-gnu](https://hub.docker.com/repository/docker/adlinktech/zenoh-dev-manylinux2014-aarch64-gnu)


A [manylinux2014](https://github.com/pypa/manylinux)-based image to target most of the Linux ARM aarch64 platforms.  
It includes:
  * To build zenoh and its backends:
    - Rust with the nightly toolchain by default and `x86_64-unknown-linux-gnu` as default target
    - openssl-devel (required by InfluxDB backend)
    - clang-devel llvm-devel (required by the file system backend, because of rocksdb dependency)
    - dpkg rpm-build (for debian packaging)
  * To build zenoh-python:
    - All the Python versions provided by manylinux2014
    - maturin
  * To build zenoh-c:
    - cbindgen
  * To build zenoh-pico:
    - cmake3

Usage to build zenoh:
```bash
   docker run --init --rm -v $(pwd):/workdir -w /workdir adlinktech/zenoh-dev-manylinux2014-aarch64-gnu cargo build --release --bins --lib --examples
```

# [zenoh-dev-x86_64-unknown-linux-musl](https://hub.docker.com/repository/docker/adlinktech/zenoh-dev-x86_64-unknown-linux-musl)

An [Alpine](https://hub.docker.com/_/alpine/)-based image to target Alpine itself, and then to build the eclipse/zenoh docker image running the zenoh router.  
It includes:
  * To build zenoh and its backends:
    - Rust with the nightly toolchain by default and `x86_64-unknown-linux-musl` as default target
    - gcc, musl-dev
    - openssl-dev (required by InfluxDB backend)
    - llvm9-dev clang-dev g++ linux-headers (required by the file system backend, because of rocksdb dependency)

Usage to build zenoh:
```bash
   docker run --init --rm -v $(pwd):/workdir -w /workdir adlinktech/zenoh-dev-manylinux2014-aarch64-gnu cargo build --release --bins --lib --examples
```
