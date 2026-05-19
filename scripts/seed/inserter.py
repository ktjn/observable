from __future__ import annotations

import clickhouse_connect
from tqdm import tqdm


class Inserter:
    def __init__(self, host: str, port: int, user: str, password: str, batch_size: int = 10_000):
        self._client = clickhouse_connect.get_client(
            host=host,
            port=port,
            username=user,
            password=password,
            database="observable",
        )
        self._batch_size = batch_size

    def flush(self, table: str, column_names: list[str], rows: list[list]) -> int:
        """Insert rows into table. Returns number of rows inserted."""
        if not rows:
            return 0
        self._client.insert(table, rows, column_names=column_names)
        return len(rows)

    def insert_in_batches(
        self,
        table: str,
        column_names: list[str],
        rows: list[list],
        desc: str = "",
    ) -> int:
        """Insert all rows in batches, showing tqdm progress. Returns total inserted."""
        if not rows:
            return 0
        total = 0
        with tqdm(total=len(rows), desc=desc or table, unit="rows", leave=False) as pbar:
            for i in range(0, len(rows), self._batch_size):
                batch = rows[i : i + self._batch_size]
                self.flush(table, column_names, batch)
                total += len(batch)
                pbar.update(len(batch))
        return total

    def row_count(self, table: str, tenant_id: str) -> int:
        """Return row count for a tenant in the given table (used for --resume)."""
        result = self._client.query(
            f"SELECT count() FROM {table} WHERE tenant_id = %(tid)s",
            parameters={"tid": tenant_id},
        )
        return result.first_row[0]

    def close(self) -> None:
        self._client.close()
