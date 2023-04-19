# Copyright (c) 2023 ZettaScale Technology
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
### Dockerfile creating the eclipse/zenoh image from cross-compiled binaries.
### It assumes that zenohd and it's plugins libraries are installed in docker/$TARGETPLATFORM/
### where $TARGETPLATFORM is set by buildx to a Docker supported platform such as linux/amd64 or linux/arm64
### (see https://docs.docker.com/buildx/working-with-buildx/#build-multi-platform-images)
###


FROM alpine:latest

ARG TARGETPLATFORM

RUN apk add --no-cache libgcc libstdc++

COPY docker/$TARGETPLATFORM/zenohd /
COPY docker/$TARGETPLATFORM/*.so /

RUN echo '#!/bin/ash' > /entrypoint.sh
RUN echo 'echo " * Starting: /zenohd $*"' >> /entrypoint.sh
RUN echo 'exec /zenohd $*' >> /entrypoint.sh
RUN chmod +x /entrypoint.sh

EXPOSE 7446/udp
EXPOSE 7447/tcp
EXPOSE 8000/tcp

ENV RUST_LOG info

ENTRYPOINT ["/entrypoint.sh"]
