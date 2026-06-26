# CorrelatedEvent v1

**Domain:** pipeline  
**Name:** CorrelatedEvent  
**Version:** 1  
**Artifact ID:** pipeline.CorrelatedEvent.v1  
**Artifact:** pipeline.CorrelatedEvent.v1.md  
**Owner:** crypto-demo-team  
**Kind:** entity  
**Change kind:** additive  

## Fields

| Field | Type | Required | Default | Annotations | Classification |
|---|---|---|---|---|---|
| correlationId | uuid | yes | — | @key | — |
| asset | string | yes | — | — | — |
| txHash | string | yes | — | — | — |
| priceUsd | float | yes | — | — | — |
| lagMs | int | yes | — | — | — |
| priceSource | enum(DexPaprika, Coinbase) | yes | — | @wire(json.case: "lowercase") | — |
| tsUnixMs | int | yes | — | — | — |
