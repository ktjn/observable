import logging
import os
import random
import time
from typing import Iterable

import httpx
from opentelemetry import metrics, trace
from opentelemetry.metrics import CallbackOptions, Observation
from opentelemetry._logs import set_logger_provider
from opentelemetry.exporter.otlp.proto.grpc._log_exporter import OTLPLogExporter
from opentelemetry.exporter.otlp.proto.grpc.metric_exporter import OTLPMetricExporter
from opentelemetry.exporter.otlp.proto.grpc.trace_exporter import OTLPSpanExporter
from opentelemetry.instrumentation.logging import LoggingInstrumentor
from opentelemetry.sdk._logs import LoggerProvider
from opentelemetry.sdk._logs.export import BatchLogRecordProcessor
from opentelemetry.sdk.metrics import MeterProvider
from opentelemetry.sdk.metrics.export import PeriodicExportingMetricReader
from opentelemetry.sdk.resources import Resource
from opentelemetry.sdk.trace import TracerProvider
from opentelemetry.sdk.trace.export import BatchSpanProcessor

logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(name)s %(message)s")
log = logging.getLogger("shop-loadgen")

OTLP_ENDPOINT = os.getenv("OTEL_EXPORTER_OTLP_ENDPOINT", "http://localhost:4317")
API_URL = os.getenv("SHOP_API_URL", "http://shop-api:8000")

resource = Resource.create({
    "service.name": "shop-loadgen",
    "service.version": os.getenv("SERVICE_VERSION", "0.1.0"),
    "k8s.pod.name": os.getenv("MY_POD_NAME", "local"),
    "k8s.namespace.name": os.getenv("MY_POD_NAMESPACE", "local"),
})

# Traces
trace_provider = TracerProvider(resource=resource)
trace_provider.add_span_processor(BatchSpanProcessor(OTLPSpanExporter(endpoint=OTLP_ENDPOINT, insecure=True)))
trace.set_tracer_provider(trace_provider)
tracer = trace.get_tracer("shop-loadgen")

# Metrics
metric_reader = PeriodicExportingMetricReader(
    OTLPMetricExporter(endpoint=OTLP_ENDPOINT, insecure=True),
    export_interval_millis=15_000,
)
meter_provider = MeterProvider(resource=resource, metric_readers=[metric_reader])
metrics.set_meter_provider(meter_provider)
meter = metrics.get_meter("shop-loadgen")

_cart_count = 0
_pending_count = 0


def observe_cart_count(_options: CallbackOptions) -> Iterable[Observation]:
    yield Observation(_cart_count)


def observe_pending_count(_options: CallbackOptions) -> Iterable[Observation]:
    yield Observation(_pending_count)


meter.create_observable_gauge(
    "shop.cart.active_count",
    callbacks=[observe_cart_count],
    unit="1",
    description="Active shopping carts",
)
meter.create_observable_gauge(
    "shop.orders.pending_count",
    callbacks=[observe_pending_count],
    unit="1",
    description="Pending orders",
)
request_counter = meter.create_counter("shop.requests.total", unit="1", description="Total loadgen requests")

# Logs
log_provider = LoggerProvider(resource=resource)
log_provider.add_log_record_processor(BatchLogRecordProcessor(OTLPLogExporter(endpoint=OTLP_ENDPOINT, insecure=True)))
set_logger_provider(log_provider)

LoggingInstrumentor().instrument(set_logging_format=True)

SCENARIOS = [
    ("browse_products", 30),
    ("place_order", 30),
    ("user_lookup", 15),
    ("check_inventory", 15),
    ("restock", 5),
    ("error_path", 5),
]
_weights = [w for _, w in SCENARIOS]
_names = [n for n, _ in SCENARIOS]

PRODUCT_IDS = list(range(1, 11))
USER_IDS = list(range(1, 6))


def pick_scenario() -> str:
    return random.choices(_names, weights=_weights, k=1)[0]


def run_browse_products(client: httpx.Client):
    with tracer.start_as_current_span("loadgen.scenario.browse_products") as span:
        resp = client.get(f"{API_URL}/products")
        span.set_attribute("http.status_code", resp.status_code)
        log.info("browse_products status=%d count=%d", resp.status_code, len(resp.json()))
        request_counter.add(1, {"scenario": "browse_products"})


def run_place_order(client: httpx.Client):
    product_id = random.choice(PRODUCT_IDS)
    user_id = random.choice(USER_IDS)
    with tracer.start_as_current_span("loadgen.scenario.place_order") as span:
        span.set_attribute("order.product_id", product_id)
        resp = client.post(f"{API_URL}/orders", json={"product_id": product_id, "user_id": user_id})
        span.set_attribute("http.status_code", resp.status_code)
        if resp.status_code == 201:
            log.info("place_order product_id=%d order_id=%s", product_id, resp.json().get("order_id"))
        elif resp.status_code == 409:
            log.warning("place_order rejected product_id=%d reason=out_of_stock", product_id)
        else:
            log.warning("place_order failed product_id=%d status=%d", product_id, resp.status_code)
        request_counter.add(1, {"scenario": "place_order"})


def run_user_lookup(client: httpx.Client):
    user_id = random.choice(USER_IDS)
    with tracer.start_as_current_span("loadgen.scenario.user_lookup") as span:
        span.set_attribute("user.id", user_id)
        resp = client.get(f"{API_URL}/users/{user_id}")
        span.set_attribute("http.status_code", resp.status_code)
        log.info("user_lookup user_id=%d status=%d", user_id, resp.status_code)
        request_counter.add(1, {"scenario": "user_lookup"})


def run_check_inventory(client: httpx.Client):
    with tracer.start_as_current_span("loadgen.scenario.check_inventory") as span:
        resp = client.get(f"{API_URL}/inventory")
        span.set_attribute("http.status_code", resp.status_code)
        if resp.status_code == 200:
            items = resp.json()
            low_stock = [i for i in items if i["quantity"] < 10]
            span.set_attribute("inventory.items_total", len(items))
            span.set_attribute("inventory.items_low_stock", len(low_stock))
            log.info("check_inventory items=%d low_stock=%d", len(items), len(low_stock))
        request_counter.add(1, {"scenario": "check_inventory"})


def run_restock(client: httpx.Client):
    product_id = random.choice(PRODUCT_IDS)
    with tracer.start_as_current_span("loadgen.scenario.restock") as span:
        span.set_attribute("product.id", product_id)
        resp = client.post(f"{API_URL}/inventory/restock/{product_id}")
        span.set_attribute("http.status_code", resp.status_code)
        if resp.status_code == 200:
            new_qty = resp.json().get("quantity")
            span.set_attribute("inventory.quantity_after", new_qty)
            log.info("restock product_id=%d new_quantity=%d", product_id, new_qty)
        request_counter.add(1, {"scenario": "restock"})


def run_error_path(client: httpx.Client):
    with tracer.start_as_current_span("loadgen.scenario.error_path") as span:
        span.set_attribute("error.intentional", True)
        bad_id = random.randint(9000, 9999)
        resp = client.get(f"{API_URL}/products/{bad_id}")
        span.set_attribute("http.status_code", resp.status_code)
        if resp.status_code == 404:
            log.error("error_path intentional 404 product_id=%d", bad_id)
        else:
            log.warning("error_path unexpected status=%d", resp.status_code)
        request_counter.add(1, {"scenario": "error_path"})


RUNNERS = {
    "browse_products": run_browse_products,
    "place_order": run_place_order,
    "user_lookup": run_user_lookup,
    "check_inventory": run_check_inventory,
    "restock": run_restock,
    "error_path": run_error_path,
}


def emit_gauge_metrics():
    global _cart_count, _pending_count
    _cart_count = random.randint(0, 200)
    _pending_count = random.randint(0, 50)


def wait_for_api(client: httpx.Client):
    for attempt in range(30):
        try:
            r = client.get(f"{API_URL}/health", timeout=3)
            if r.status_code == 200:
                log.info("shop-api is ready")
                return
        except Exception:
            pass
        log.info("waiting for shop-api (%d/30)", attempt + 1)
        time.sleep(5)
    log.warning("shop-api did not become ready; proceeding anyway")


def main():
    with httpx.Client(timeout=10.0) as client:
        wait_for_api(client)
        last_metric_emit = 0.0
        iteration = 0

        while True:
            scenario = pick_scenario()
            try:
                RUNNERS[scenario](client)
            except Exception as exc:
                log.error("scenario %s failed: %s", scenario, exc)

            iteration += 1
            now = time.monotonic()
            if now - last_metric_emit >= 15:
                emit_gauge_metrics()
                last_metric_emit = now

            # Poisson-distributed sleep, mean 5s, clamped 1–30s
            delay = min(30.0, max(1.0, random.expovariate(1 / 5)))
            log.info("iteration=%d scenario=%s next_delay=%.1fs", iteration, scenario, delay)
            time.sleep(delay)


if __name__ == "__main__":
    main()
