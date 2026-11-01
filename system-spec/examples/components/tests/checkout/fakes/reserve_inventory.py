"""Fake of the reserve-inventory contract (spec.yaml): 409 insufficient_inventory when out of stock."""


def reserve(product_id: str, qty: int) -> bool:
    return product_id == "in-stock" and qty <= 3
