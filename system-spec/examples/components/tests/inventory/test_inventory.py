from src.inventory.api import reserve


def test_reserve_never_goes_negative():
    assert reserve("p1", 99) is False


def test_reserve_contract():
    """Provider contract test for reserve-inventory: the behavior checkout's fake models (insufficient_inventory when stock is short)."""
    assert reserve("p1", 1000) is False
