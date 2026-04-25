import logging
import os
import random
import time

import asyncpg
from aio_pika import connect_robust, Message
from fastapi import FastAPI, HTTPException, Response
from opentelemetry import trace
from opentelemetry.exporter.otlp.proto.grpc.trace_exporter import OTLPSpanExporter
from opentelemetry.instrumentation.asyncpg import AsyncPGInstrumentor
from opentelemetry.instrumentation.fastapi import FastAPIInstrumentor
from opentelemetry.instrumentation.logging import LoggingInstrumentor
from opentelemetry.sdk.resources import Resource
from opentelemetry.sdk.trace import TracerProvider
from opentelemetry.sdk.trace.export import BatchSpanProcessor

logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(name)s %(message)s")
log = logging.getLogger("shop-api")

# OTel setup
resource = Resource.create({
    "service.name": "shop-api",
    "service.version": os.getenv("SERVICE_VERSION", "0.1.0"),
    "k8s.pod.name": os.getenv("MY_POD_NAME", "local"),
    "k8s.namespace.name": os.getenv("MY_POD_NAMESPACE", "local"),
})
provider = TracerProvider(resource=resource)
exporter = OTLPSpanExporter(endpoint=os.getenv("OTEL_EXPORTER_OTLP_ENDPOINT", "http://localhost:4317"), insecure=True)
provider.add_span_processor(BatchSpanProcessor(exporter))
trace.set_tracer_provider(provider)
tracer = trace.get_tracer("shop-api")

LoggingInstrumentor().instrument(set_logging_format=True)
AsyncPGInstrumentor().instrument()

app = FastAPI(title="shop-api")
FastAPIInstrumentor.instrument_app(app)

DB_DSN = os.getenv("DATABASE_URL", "postgresql://shop:shop@shop-db:5432/shop")
AMQP_URL = os.getenv("AMQP_URL", "amqp://shop:shop@shop-queue:5672/")

_pool: asyncpg.Pool | None = None
_amqp = None
_channel = None


@app.on_event("startup")
async def startup():
    global _pool, _amqp, _channel
    for attempt in range(20):
        try:
            _pool = await asyncpg.create_pool(DB_DSN, min_size=2, max_size=10)
            break
        except Exception:
            log.warning("DB not ready, retrying (%d/20)", attempt + 1)
            time.sleep(3)
    _amqp = await connect_robust(AMQP_URL)
    _channel = await _amqp.channel()
    await _channel.declare_queue("orders", durable=True)
    log.info("shop-api started")


@app.on_event("shutdown")
async def shutdown():
    if _pool:
        await _pool.close()
    if _amqp:
        await _amqp.close()


@app.get("/health")
async def health():
    return {"status": "ok"}


@app.get("/products")
async def list_products():
    async with _pool.acquire() as conn:
        rows = await conn.fetch("SELECT id, name, price FROM products ORDER BY id LIMIT 20")
    log.info("products listed count=%d", len(rows))
    return [dict(r) for r in rows]


@app.get("/products/{product_id}")
async def get_product(product_id: int):
    async with _pool.acquire() as conn:
        row = await conn.fetchrow("SELECT id, name, price FROM products WHERE id = $1", product_id)
    if not row:
        log.warning("product not found id=%d", product_id)
        raise HTTPException(status_code=404, detail="product not found")
    return dict(row)


@app.post("/orders", status_code=201)
async def place_order(body: dict):
    with tracer.start_as_current_span("order.place") as span:
        product_id = body.get("product_id")
        user_id = body.get("user_id", 1)
        if not product_id:
            raise HTTPException(status_code=400, detail="product_id required")

        async with _pool.acquire() as conn:
            product = await conn.fetchrow("SELECT id, price FROM products WHERE id = $1", product_id)
            if not product:
                span.set_attribute("error", True)
                raise HTTPException(status_code=404, detail="product not found")
            order_id = await conn.fetchval(
                "INSERT INTO orders (user_id, product_id, total, status) VALUES ($1, $2, $3, 'pending') RETURNING id",
                user_id, product_id, product["price"],
            )

        span.set_attribute("order.id", order_id)
        span.set_attribute("order.product_id", product_id)

        await _channel.default_exchange.publish(
            Message(f'{{"order_id": {order_id}, "product_id": {product_id}}}'.encode()),
            routing_key="orders",
        )
        log.info("order placed order_id=%d product_id=%d", order_id, product_id)
        return {"order_id": order_id}


@app.get("/users/{user_id}")
async def get_user(user_id: int):
    async with _pool.acquire() as conn:
        row = await conn.fetchrow("SELECT id, name, email FROM users WHERE id = $1", user_id)
    if not row:
        log.warning("user not found id=%d", user_id)
        raise HTTPException(status_code=404, detail="user not found")
    log.info("user fetched user_id=%d", user_id)
    return dict(row)
