# Kubernetes Deployment

## Helm Chart

Install Mnemo on Kubernetes using the Helm chart:

```bash
helm install mnemo deploy/helm/mnemo/ \
  --set postgres.url="postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@postgres:5432/mnemo" \
  --set openaiApiKey="${OPENAI_API_KEY}"
```

## Configuration

Key `values.yaml` options:

| Value | Default | Description |
|-------|---------|-------------|
| `replicaCount` | 1 | Number of replicas |
| `image.repository` | `ghcr.io/mnemo-ai/mnemo` | Container image |
| `image.tag` | `latest` | Image tag |
| `postgres.url` | - | PostgreSQL connection URL |
| `openaiApiKey` | - | OpenAI API key |
| `rest.enabled` | true | Enable REST API |
| `rest.port` | 8080 | REST API port |
| `resources.requests.cpu` | 100m | CPU request |
| `resources.requests.memory` | 128Mi | Memory request |
| `ingress.enabled` | false | Enable ingress |

## Scaling

For production with PostgreSQL:

```bash
helm upgrade mnemo deploy/helm/mnemo/ --set replicaCount=3
```

Multiple replicas share the same PostgreSQL database, each handling MCP or REST connections independently.
