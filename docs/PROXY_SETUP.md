# WebTransport Proxy Setup Guide

## Why a Proxy?
Browser WebTransport requires HTTPS termination. Use a reverse proxy to:
1. Handle TLS certificates
2. Convert WebTransport → plain UDP
3. Forward to SPORE daemon

## Caddy Configuration
```caddy
# Caddyfile
proxy.spore.example {
  reverse_proxy https://spore-daemon.internal {
    transport http {
      versions h3
    }
    rewrite /spore-transport /udp/127.0.0.1:7439
  }
}
```

## Nginx Configuration
```nginx
# nginx.conf
server {
  listen 443 ssl http2;
  server_name proxy.spore.example;
  
  ssl_certificate /path/to/cert.pem;
  ssl_certificate_key /path/to/key.pem;

  location /spore-transport {
    proxy_pass http://127.0.0.1:7439/udp;
    proxy_http_version 3.0;
  }
}
```

## Production Notes
1. Use Let's Encrypt for certificates
2. Bind to port 443 (browsers require standard HTTPS port)
3. Configure firewall to allow UDP 7439
4. For development, use self-signed certificates with `#certhash` in URL:
   `https://localhost:4433/spore-transport#sha-256=...`