"""Checkout tests: depend on a fake of the required reserve-inventory contract, never on inventory code."""
from src.checkout.app import place_order
from tests.checkout.fakes.reserve_inventory import reserve


def test_place_order():
    assert place_order("in-stock", 1, reserve=reserve)["status"] == 201


def test_orders_api():
    assert place_order("in-stock", 9, reserve=reserve) == {"status": 409, "error": "insufficient_inventory"}


def test_order_serialization():
    assert set(place_order("in-stock", 1, reserve=reserve)["order"]) == {"product_id", "qty"}


def test_inventory_retry():
    assert place_order("other", 1, reserve=reserve)["status"] == 409  # illustrative: retry not implemented
