import importlib.util
import os
from pathlib import Path


def load_mock():
    path = Path(os.environ["SYSTEM_SPEC_MOCK"])
    spec = importlib.util.spec_from_file_location("programmable_provider", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.mock


def checkout_standalone():
    from src.checkout.app import place_order
    from tests.checkout.fakes.reserve_inventory import reserve
    assert place_order("in-stock", 1, reserve)["status"] == 201
    assert place_order("in-stock", 9, reserve)["status"] == 409


def inventory_standalone():
    from src.inventory.api import reserve, STOCK
    STOCK.clear()
    STOCK["p1"] = 1
    assert reserve("p1", 1)
    assert not reserve("p1", 1)


def inventory_contract(mode):
    if mode == "real":
        from src.inventory.api import reserve, STOCK
        STOCK.clear()
        STOCK["p1"] = 1
        assert reserve("p1", 1)
        assert not reserve("p1", 1)
    else:
        mock = load_mock()
        mock.configure(True, False)
        assert mock.reserve("p1", 1)
        assert not mock.reserve("p1", 1)
        assert mock.calls == [("p1", 1), ("p1", 1)]
        mock.reset()
        assert mock.calls == []
        mock.configure(TimeoutError("timeout"))
        try:
            mock.reserve("p1", 1)
        except TimeoutError:
            pass
        else:
            raise AssertionError("timeout was not simulated")
        mock.reset()
        try:
            mock.reserve("p1", 1)
        except AssertionError:
            pass
        else:
            raise AssertionError("unconfigured interaction must fail")


def checkout_contract(mode):
    if mode == "real":
        from src.checkout.app import place_order
        def reserve_yes(product_id, qty): return True
        def reserve_no(product_id, qty): return False
        assert place_order("p1", 1, reserve_yes)["status"] == 201
        assert place_order("p1", 1, reserve_no)["status"] == 409
    else:
        mock = load_mock()
        mock.configure({"status": 201, "order": {"product_id": "p1", "qty": 1}},
                       {"status": 409, "error": "insufficient_inventory"})
        assert mock.place_order("p1", 1, lambda *_: True)["status"] == 201
        assert mock.place_order("p1", 1, lambda *_: False)["status"] == 409
        assert len(mock.calls) == 2
        mock.reset()
        assert mock.calls == []
        mock.configure(TimeoutError("timeout"))
        try:
            mock.place_order("p1", 1, lambda *_: True)
        except TimeoutError:
            pass
        else:
            raise AssertionError("timeout was not simulated")


component = os.environ["SYSTEM_SPEC_COMPONENT"]
mode = os.environ["SYSTEM_SPEC_MODE"]
if mode == "standalone":
    (checkout_standalone if component == "checkout" else inventory_standalone)()
else:
    (checkout_contract if component == "checkout" else inventory_contract)(mode)
