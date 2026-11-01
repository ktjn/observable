FROM ghcr.io/astral-sh/uv:python3.12-bookworm-slim
ENV UV_PROJECT_ENVIRONMENT=/opt/venv PATH=/opt/venv/bin:$PATH
WORKDIR /app
COPY pyproject.toml uv.lock ./
RUN uv sync --frozen --no-install-project
COPY . .
CMD ["python", "metamodel.py", "example.yaml"]
