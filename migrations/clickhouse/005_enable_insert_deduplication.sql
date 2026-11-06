ALTER TABLE observable.spans MODIFY SETTING non_replicated_deduplication_window = 1000;
ALTER TABLE observable.span_events MODIFY SETTING non_replicated_deduplication_window = 1000;
ALTER TABLE observable.logs MODIFY SETTING non_replicated_deduplication_window = 1000;
ALTER TABLE observable.metric_series MODIFY SETTING non_replicated_deduplication_window = 1000;
ALTER TABLE observable.metric_points MODIFY SETTING non_replicated_deduplication_window = 1000;
