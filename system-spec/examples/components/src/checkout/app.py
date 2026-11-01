"""Checkout component: provides place-order, requires reserve-inventory (declared in spec.yaml)."""
from src.inventory import api


def place_order(product_id: str, qty: int, reserve=api.reserve) -> dict:
    if not reserve(product_id, qty):
        return {"status": 409, "error": "insufficient_inventory"}
    return {"status": 201, "order": {"product_id": product_id, "qty": qty}}
