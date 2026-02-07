# Deployment

Mnemo can be deployed in several configurations:

| Mode | Backend | Best For |
|------|---------|----------|
| **Embedded** | DuckDB | Single-agent, local development |
| **Distributed** | PostgreSQL | Multi-agent, production |
| **Docker** | Either | Container deployments |
| **Kubernetes** | PostgreSQL | Scalable production |

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `MNEMO_DB_PATH` | DuckDB database path | `mnemo.db` |
| `MNEMO_POSTGRES_URL` | PostgreSQL connection URL | - |
| `MNEMO_REST_PORT` | REST API port | - |
| `MNEMO_AGENT_ID` | Default agent ID | `default` |
| `MNEMO_ORG_ID` | Organization ID | - |
| `OPENAI_API_KEY` | OpenAI API key for embeddings | - |
| `MNEMO_EMBEDDING_MODEL` | Embedding model name | `text-embedding-3-small` |
| `MNEMO_DIMENSIONS` | Embedding dimensions | `1536` |
