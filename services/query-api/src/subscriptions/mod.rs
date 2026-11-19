//! Server-managed read-only view subscriptions: `GET /v1/ui/subscriptions/events`
//! and `POST /v1/ui/subscriptions/{id}/ack` (ADR-046, PUSH-002/006..008/016..022).

pub mod ack;
pub mod events;
pub mod hub;
pub mod protocol;
pub mod replay;
pub mod stream;
pub mod views;
