# Docker Deployment

## Quick Start

```bash
docker run -d \
  --name mnemo \
  -v mnemo-data:/data \
  -e MNEMO_DB_PATH=/data/mnemo.db \
  -e OPENAI_API_KEY=sk-... \
  ghcr.io/mnemo-ai/mnemo:latest
```

## Docker Compose

```bash
docker-compose up -d
```

The included `docker-compose.yml` starts Mnemo with PostgreSQL:

```yaml
services:
  mnemo:
    build: .
    environment:
      MNEMO_POSTGRES_URL: postgres://mnemo:mnemo@postgres/mnemo
      OPENAI_API_KEY: ${OPENAI_API_KEY}
      MNEMO_REST_PORT: "8080"
    ports:
      - "8080:8080"
    depends_on:
      - postgres

  postgres:
    image: pgvector/pgvector:pg16
    environment:
      POSTGRES_USER: mnemo
      POSTGRES_PASSWORD: mnemo
      POSTGRES_DB: mnemo
    volumes:
      - pg-data:/var/lib/postgresql/data
```

## Building the Image

```bash
docker build -t mnemo .
```

The Dockerfile uses a multi-stage build for minimal image size.
