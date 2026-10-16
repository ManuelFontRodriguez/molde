# The only way out of the agent's network: an HTTP(S) proxy that allows the hosts in
# allow.txt and refuses everything else. The agent container sits on an internal Docker
# network with no route out; it reaches the internet only through this proxy.
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends tinyproxy && rm -rf /var/lib/apt/lists/*
COPY tinyproxy.conf /etc/tinyproxy/tinyproxy.conf
COPY allow.txt /etc/tinyproxy/allow.txt
EXPOSE 8888
CMD ["tinyproxy", "-d", "-c", "/etc/tinyproxy/tinyproxy.conf"]
