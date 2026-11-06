"""Checkout component: provides place-order, requires an injected reserve-inventory capability."""


def place_order(product_id: str, qty: int, reserve) -> dict:
    if not reserve(product_id, qty):
        return {"status": 409, "error": "insufficient_inventory"}
    return {"status": 201, "order": {"product_id": product_id, "qty": qty}}
