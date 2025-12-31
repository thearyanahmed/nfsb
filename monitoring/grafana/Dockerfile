FROM grafana/grafana:10.2.0

# copy provisioning configs
COPY provisioning/datasources/datasources.yml /etc/grafana/provisioning/datasources/
COPY provisioning/dashboards/dashboards.yml /etc/grafana/provisioning/dashboards/

# copy dashboards
COPY dashboards/ /var/lib/grafana/dashboards/

# copy entrypoint
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

ENV GF_AUTH_ANONYMOUS_ENABLED=true
ENV GF_AUTH_ANONYMOUS_ORG_ROLE=Viewer
ENV GF_SECURITY_ADMIN_PASSWORD=admin

EXPOSE 3000

ENTRYPOINT ["/entrypoint.sh"]
