FROM ghcr.io/muchobien/pocketbase:latest

USER root

RUN apk add --no-cache curl jq

COPY entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh

ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
