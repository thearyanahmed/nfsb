FROM grafana/grafana:10.0.0

# copy datasource provisioning (will be updated at runtime via env var)
COPY provisioning/datasources/datasource.yml /etc/grafana/provisioning/datasources/datasource.yml

# copy dashboard provisioning
COPY provisioning/dashboards/dashboards.yml /etc/grafana/provisioning/dashboards/dashboards.yml

# copy dashboards
COPY dashboards/ /var/lib/grafana/dashboards/

EXPOSE 3000
