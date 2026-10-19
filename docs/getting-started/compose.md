# Vyoma Compose Guide

Vyoma Compose enables multi-VM application orchestration using standard YAML definitions. It supports dependency resolution, environment variable injection, port exposure, and horizontal replica scaling.

---

## 1. Defining `vyoma-compose.yml`

Vyoma supports the modern Docker Compose v3 schema:

```yaml
version: "3.8"

services:
  web:
    image: nginx:alpine
    ports:
      - "8080:80"
    volumes:
      - "./html:/usr/share/nginx/html"
    cpus: 1
    memory: 512
    depends_on:
      - api

  api:
    image: python:3.11-slim
    ports:
      - "5000:5000"
    environment:
      DATABASE_URL: "postgres://postgres:secret@db:5432/app"
    depends_on:
      - db

  db:
    image: postgres:15-alpine
    environment:
      POSTGRES_PASSWORD: "secret"
    memory: 1024
```

---

## 2. Managing the Stack

### Launching Services (`vyoma up`)

Start all services with dependency-aware ordering:

```bash
vyoma up -d
```

Output:
```text
[+] Resolving dependency tree...
[+] Starting db (postgres:15-alpine)... Done (IP: 172.16.0.2)
[+] Starting api (python:3.11-slim)... Done (IP: 172.16.0.3)
[+] Starting web (nginx:alpine)... Done (IP: 172.16.0.4)
Stack 'my-app' deployed successfully.
```

### Scaling Services (`vyoma scale`)

Scale instances dynamically with automatic round-robin internal DNS:

```bash
vyoma scale api=3
```

Now, DNS requests to `api.vyoma` will automatically resolve round-robin across all 3 running worker VMs.

### Stopping the Stack (`vyoma down`)

Stop and remove all associated Micro-VMs and virtual networking interfaces:

```bash
vyoma down
```

---

## 3. Service Discovery

All services within a Compose stack are automatically connected to the internal bridge network. The daemon's embedded DNS server (`172.16.0.1:53`) registers every service by name.

For instance, the `api` VM can directly ping or connect to `db` without knowing its allocated IP address:

```bash
vyoma exec api ping -c 2 db
```
