# Observable Platform

Full-stack observability platform specification.

## Documentation

The full specification is located in the [spec/](spec/) directory.

Implementation plans and iteration documents can be found in [docs/superpowers/plans/](docs/superpowers/plans/).

## AI Agent Instructions

Mandatory instructions for any AI agent interacting with this repository can be found in:
- [AGENT.md](AGENT.md) (Generic)
- [GEMINI.md](GEMINI.md) (Gemini CLI)
- [CLAUDE.md](CLAUDE.md) (Claude Desktop/CLI)

## Development

The entire stack can be started with Docker Compose. This will build the services, run migrations, and start the system.

```bash
# Start the full local stack
docker compose up -d

# Open the frontend
# http://localhost:5173

# Run smoke tests
docker compose up smoke-test --abort-on-container-exit
```

See [spec/10-process.md](spec/10-process.md) for the official development process and engineering standards.
