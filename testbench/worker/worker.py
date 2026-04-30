import json
import logging
import os
import random
import time

import pika
import psycopg2
from opentelemetry import metrics, trace
from opentelemetry.exporter.otlp.proto.grpc.metric_exporter import OTLPMetricExporter
from opentelemetry.exporter.otlp.proto.grpc.trace_exporter import OTLPSpanExporter
from opentelemetry.instrumentation.logging import LoggingInstrumentor
from opentelemetry.instrumentation.pika import PikaInstrumentor
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
order_duration_gauge = meter.create_gauge(
    "order_processing_duration_ms",
    unit="ms",
    description="Order processing duration in milliseconds",
)

LoggingInstrumentor().instrument(set_logging_format=True)
PikaInstrumentor().instrument()

AMQP_URL = os.getenv("AMQP_URL", "amqp://shop:shop@shop-queue:5672/")
DB_DSN = os.getenv("DATABASE_URL", "postgresql://shop:shop@shop-db:5432/shop")


def get_db():
    for attempt in range(20):
        try:
            return psycopg2.connect(DB_DSN)
        except Exception:
            log.warning("DB not ready, retrying (%d/20)", attempt + 1)
            time.sleep(3)
    raise RuntimeError("could not connect to database")


def process_order(ch, method, properties, body):
    with tracer.start_as_current_span("worker.process_order") as span:
        t0 = time.monotonic()
        try:
            payload = json.loads(body)
            order_id = payload["order_id"]
            span.set_attribute("order.id", order_id)

            delay = random.uniform(0.5, 2.0)
            time.sleep(delay)

            db = get_db()
            cur = db.cursor()
            cur.execute("UPDATE orders SET status = 'processed' WHERE id = %s", (order_id,))
            db.commit()
            cur.close()
            db.close()

            duration_ms = (time.monotonic() - t0) * 1000.0
            order_duration_gauge.set(
                duration_ms,
                attributes={"service.name": "shop-worker", "order.status": "processed"},
            )
            ch.basic_ack(delivery_tag=method.delivery_tag)
            log.info("order processed order_id=%d delay=%.2fs", order_id, delay)
        except Exception as exc:
            span.record_exception(exc)
            span.set_attribute("error", True)
            duration_ms = (time.monotonic() - t0) * 1000.0
            order_duration_gauge.set(
                duration_ms,
                attributes={"service.name": "shop-worker", "order.status": "failed"},
            )
            log.error("failed to process order: %s", exc)
            ch.basic_nack(delivery_tag=method.delivery_tag, requeue=False)


def main():
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
