import json
import logging
import os
import random
import time

import pika
import psycopg2
from opentelemetry import metrics, trace
from opentelemetry._logs import set_logger_provider
from opentelemetry.exporter.otlp.proto.grpc._log_exporter import OTLPLogExporter
from opentelemetry.exporter.otlp.proto.grpc.metric_exporter import OTLPMetricExporter
from opentelemetry.exporter.otlp.proto.grpc.trace_exporter import OTLPSpanExporter
from opentelemetry.instrumentation.logging import LoggingInstrumentor
from opentelemetry.instrumentation.pika import PikaInstrumentor
from opentelemetry.instrumentation.psycopg2 import Psycopg2Instrumentor
from opentelemetry.sdk._logs import LoggerProvider
from opentelemetry.sdk._logs.export import BatchLogRecordProcessor
from opentelemetry.sdk.metrics import MeterProvider
from opentelemetry.sdk.metrics.export import PeriodicExportingMetricReader
from opentelemetry.sdk.resources import Resource
from opentelemetry.sdk.trace import TracerProvider
from opentelemetry.sdk.trace.export import BatchSpanProcessor

logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(name)s %(message)s")
log = logging.getLogger("shop-worker")

OTLP_ENDPOINT = os.getenv("OTEL_EXPORTER_OTLP_ENDPOINT", "http://localhost:4317")

resource = Resource.create({
    "service.name": "shop-worker",
    "service.version": os.getenv("SERVICE_VERSION", "0.1.0"),
    "k8s.pod.name": os.getenv("MY_POD_NAME", "local"),
    "k8s.namespace.name": os.getenv("MY_POD_NAMESPACE", "local"),
})
provider = TracerProvider(resource=resource)
exporter = OTLPSpanExporter(endpoint=OTLP_ENDPOINT, insecure=True)
provider.add_span_processor(BatchSpanProcessor(exporter))
trace.set_tracer_provider(provider)
tracer = trace.get_tracer("shop-worker")

metric_reader = PeriodicExportingMetricReader(
    OTLPMetricExporter(endpoint=OTLP_ENDPOINT, insecure=True),
    export_interval_millis=15_000,
)
meter_provider = MeterProvider(resource=resource, metric_readers=[metric_reader])
metrics.set_meter_provider(meter_provider)
meter = metrics.get_meter("shop-worker")
order_duration_histogram = meter.create_histogram(
    "order_processing_duration_ms",
    unit="ms",
    description="Order processing duration in milliseconds",
)
orders_processed_counter = meter.create_counter(
    "shop.orders.processed_total",
    unit="1",
    description="Total orders processed by the worker",
)

log_provider = LoggerProvider(resource=resource)
log_provider.add_log_record_processor(
    BatchLogRecordProcessor(OTLPLogExporter(endpoint=OTLP_ENDPOINT, insecure=True))
)
set_logger_provider(log_provider)

LoggingInstrumentor().instrument(set_logging_format=True)
PikaInstrumentor().instrument()
Psycopg2Instrumentor().instrument()

AMQP_URL = os.getenv("AMQP_URL", "amqp://shop:shop@shop-queue:5672/")
DB_DSN = os.getenv("DATABASE_URL", "postgresql://shop:shop@shop-db:5432/shop")

_db: psycopg2.extensions.connection | None = None


def get_db() -> psycopg2.extensions.connection:
    global _db
    if _db is None or _db.closed:
        for attempt in range(20):
            try:
                _db = psycopg2.connect(DB_DSN)
                log.info("connected to database")
                return _db
            except Exception:
                log.warning("DB not ready, retrying (%d/20)", attempt + 1)
                time.sleep(3)
        raise RuntimeError("could not connect to database")
    return _db


def process_order(ch, method, properties, body):
    with tracer.start_as_current_span("worker.process_order") as span:
        t0 = time.monotonic()
        status = "failed"
        try:
            payload = json.loads(body)
            order_id = payload["order_id"]
            product_id = payload.get("product_id")
            span.set_attribute("order.id", order_id)
            if product_id:
                span.set_attribute("order.product_id", product_id)

            delay = random.uniform(0.5, 2.0)
            time.sleep(delay)

            conn = get_db()
            cur = conn.cursor()

            cur.execute(
                "SELECT id, user_id, product_id, total, status FROM orders WHERE id = %s",
                (order_id,),
            )
            row = cur.fetchone()
            if row is None:
                raise ValueError(f"order {order_id} not found in database")

            cur.execute(
                "UPDATE orders SET status = 'processed' WHERE id = %s AND status = 'pending'",
                (order_id,),
            )
            conn.commit()
            cur.close()

            status = "processed"
            duration_ms = (time.monotonic() - t0) * 1000.0
            order_duration_histogram.record(duration_ms, {"order.status": status})
            orders_processed_counter.add(1, {"order.status": status})
            ch.basic_ack(delivery_tag=method.delivery_tag)
            log.info("order processed order_id=%d delay=%.2fs", order_id, delay)

        except Exception as exc:
            span.record_exception(exc)
            span.set_attribute("error", True)
            duration_ms = (time.monotonic() - t0) * 1000.0
            order_duration_histogram.record(duration_ms, {"order.status": status})
            orders_processed_counter.add(1, {"order.status": status})
            log.error("failed to process order: %s", exc)
            ch.basic_nack(delivery_tag=method.delivery_tag, requeue=False)

            global _db
            _db = None


def main():
    get_db()

    for attempt in range(30):
        try:
            params = pika.URLParameters(AMQP_URL)
            conn = pika.BlockingConnection(params)
            break
        except Exception:
            log.warning("RabbitMQ not ready, retrying (%d/30)", attempt + 1)
            time.sleep(3)
    else:
        raise RuntimeError("could not connect to RabbitMQ")

    channel = conn.channel()
    channel.queue_declare(queue="orders", durable=True)
    channel.basic_qos(prefetch_count=1)
    channel.basic_consume(queue="orders", on_message_callback=process_order)
    log.info("shop-worker waiting for orders")
    channel.start_consuming()


if __name__ == "__main__":
    main()
