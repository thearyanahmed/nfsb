#!/bin/bash
set -e

# replace placeholder with actual Prometheus URL from environment
PROM_URL="${PROMETHEUS_INTERNAL_URL:-http://prometheus:9090}"
sed -i "s|PROMETHEUS_URL_PLACEHOLDER|${PROM_URL}|g" /etc/grafana/provisioning/datasources/datasources.yml

echo "Configured Prometheus datasource URL: ${PROM_URL}"

# start grafana
exec /run.sh
