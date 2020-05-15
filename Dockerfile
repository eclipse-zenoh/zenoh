FROM ubuntu:20.04

# Install required packages
RUN apt-get update
RUN apt-get install -y libev-dev libsqlite3-dev libmariadb-dev postgresql-client influxdb

# Install zenoh
RUN mkdir -p /eclipse-zenoh/bin
RUN mkdir -p /eclipse-zenoh/lib
COPY _build/default/install/bin/*.* /eclipse-zenoh/bin/
COPY _build/default/install/lib/*.* /eclipse-zenoh/lib/
RUN chmod +x /eclipse-zenoh/bin/zenohd.exe

RUN echo '#!/bin/bash' > /entrypoint.sh
RUN echo 'service influxdb start' >> /entrypoint.sh
RUN echo 'echo " * Starting: /eclipse-zenoh/bin/zenohd.exe $*"' >> /entrypoint.sh
RUN echo 'exec /eclipse-zenoh/bin/zenohd.exe $*' >> /entrypoint.sh
RUN chmod +x /entrypoint.sh

EXPOSE 7447/udp
EXPOSE 7447/tcp
EXPOSE 8000/tcp

ENTRYPOINT ["/entrypoint.sh"]
CMD [ "-v" ]
