# The homeserver the e2e suite runs against. Pinned so a Synapse release
# never breaks CI unnoticed; Dependabot proposes updates.
FROM docker.io/matrixdotorg/synapse:v1.162.0

COPY overrides.yaml /overrides.yaml

ENV SYNAPSE_SERVER_NAME=localhost \
    SYNAPSE_REPORT_STATS=no

# Generate a fresh config and signing key on every start, then run. Nothing
# is mounted from the host, so the container leaves no files behind.
ENTRYPOINT ["/bin/sh", "-c", "mkdir -p /data && /start.py generate && cat /overrides.yaml >> /data/homeserver.yaml && exec /start.py"]
