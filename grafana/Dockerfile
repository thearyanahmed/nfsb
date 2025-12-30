FROM grafana/grafana:10.2.2

# copy provisioning configs
COPY provisioning /etc/grafana/provisioning
COPY dashboards /var/lib/grafana/dashboards

# set default admin password (change in production)
ENV GF_SECURITY_ADMIN_USER=admin
ENV GF_SECURITY_ADMIN_PASSWORD=admin
ENV GF_USERS_ALLOW_SIGN_UP=false

EXPOSE 3000
