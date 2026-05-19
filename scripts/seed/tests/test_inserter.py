from unittest.mock import MagicMock, patch, call
from seed.inserter import Inserter

COLS = ["a", "b"]


def _make_inserter(batch_size=3):
    with patch("seed.inserter.clickhouse_connect.get_client", return_value=MagicMock()) as mock_get:
        ins = Inserter("localhost", 8123, "default", "pw", batch_size=batch_size)
        ins._client = MagicMock()
        return ins


def test_flush_calls_client_insert():
    ins = _make_inserter()
    rows = [[1, "x"], [2, "y"]]
    ins.flush("observable.spans", COLS, rows)
    ins._client.insert.assert_called_once_with("observable.spans", rows, column_names=COLS)


def test_flush_empty_rows_is_noop():
    ins = _make_inserter()
    ins.flush("observable.spans", COLS, [])
    ins._client.insert.assert_not_called()


def test_insert_in_batches_splits_correctly():
    ins = _make_inserter(batch_size=2)
    rows = [[i, str(i)] for i in range(5)]
    total = ins.insert_in_batches("observable.spans", COLS, rows, desc="test")
    assert total == 5
    assert ins._client.insert.call_count == 3  # batches of 2, 2, 1


def test_insert_in_batches_returns_zero_for_empty():
    ins = _make_inserter()
    total = ins.insert_in_batches("observable.spans", COLS, [], desc="empty")
    assert total == 0
    ins._client.insert.assert_not_called()
