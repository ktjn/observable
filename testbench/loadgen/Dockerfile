FROM python:3.14-slim

LABEL org.opencontainers.image.version="0.1.0"

WORKDIR /app
COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

COPY loadgen.py .

CMD ["python", "loadgen.py"]
