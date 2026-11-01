"""Inventory component: provides reserve-inventory."""

STOCK = {"p1": 3}


def reserve(product_id: str, qty: int) -> bool:
    if STOCK.get(product_id, 0) < qty:
        return False
    STOCK[product_id] -= qty
    return True
