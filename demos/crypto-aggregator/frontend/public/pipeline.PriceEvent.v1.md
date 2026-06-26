# PriceEvent v1

**Domain:** pipeline  
**Name:** PriceEvent  
**Version:** 1  
**Artifact ID:** pipeline.PriceEvent.v1  
**Artifact:** pipeline.PriceEvent.v1.md  
**Owner:** crypto-demo-team  
**Kind:** entity  
**Change kind:** additive  

## Fields

| Field | Type | Required | Default | Annotations | Classification |
|---|---|---|---|---|---|
| eventId | uuid | yes | — | @key | — |
| asset | string | yes | — | — | — |
| chain | string | yes | — | — | — |
| priceUsd | float | yes | — | — | — |
| source | enum(DexPaprika, Coinbase) | yes | — | @wire(json.case: "lowercase") | — |
| tsUnixMs | int | yes | — | — | — |
