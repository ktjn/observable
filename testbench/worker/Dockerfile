FROM python:3.12-slim

LABEL org.opencontainers.image.version="0.1.0"

WORKDIR /app
COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

COPY worker.py .

CMD ["python", "worker.py"]
