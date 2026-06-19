FROM python:3.14-slim

LABEL org.opencontainers.image.version="0.1.0"

COPY --from=ghcr.io/astral-sh/uv:latest /uv /uvx /bin/
WORKDIR /app
COPY pyproject.toml uv.lock .
RUN uv sync --locked --no-dev

COPY loadgen.py .

CMD ["uv", "run", "python", "loadgen.py"]
