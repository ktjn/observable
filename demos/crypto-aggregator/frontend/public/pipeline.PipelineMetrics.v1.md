# PipelineMetrics v1

**Domain:** pipeline  
**Name:** PipelineMetrics  
**Version:** 1  
**Artifact ID:** pipeline.PipelineMetrics.v1  
**Artifact:** pipeline.PipelineMetrics.v1.md  
**Owner:** crypto-demo-team  
**Kind:** entity  
**Change kind:** additive  

## Fields

| Field | Type | Required | Default | Annotations | Classification |
|---|---|---|---|---|---|
| snapshotId | uuid | yes | — | @key | — |
| ingestRate | float | yes | — | — | — |
| correlationLagMs | float | yes | — | — | — |
| bufferFillRatio | float | yes | — | — | — |
| exporterLatencyMs | float | yes | — | — | — |
| errorCount | int | yes | — | — | — |
| tsUnixMs | int | yes | — | — | — |
