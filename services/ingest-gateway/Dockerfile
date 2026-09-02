FROM bluenviron/mediamtx:1.20.0@sha256:86e63af28616d5e5a18540d7b031b6510bd4cbf1a3c7d224f9e2976f02aefbfb AS upstream

FROM alpine:3.22.1@sha256:4bcff63911fcb4448bd4fdacec207030997caf25e9bea4045fa6c8c44de311d1
RUN addgroup -g 10001 gateway && adduser -D -H -u 10001 -G gateway gateway
COPY --from=upstream /mediamtx /usr/local/bin/mediamtx
COPY mediamtx.yml /etc/mediamtx.yml
COPY publish-artifact.sh /usr/local/bin/publish-artifact
RUN chmod 0555 /usr/local/bin/mediamtx /usr/local/bin/publish-artifact && \
    mkdir -p /var/lib/ingest/recordings /var/lib/ingest/artifacts && \
    chown -R gateway:gateway /var/lib/ingest
USER 10001:10001
EXPOSE 8890/udp
ENTRYPOINT ["/usr/local/bin/mediamtx", "/etc/mediamtx.yml"]
