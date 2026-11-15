//! Server-managed read-only view subscriptions: `GET /v1/ui/subscriptions/events`
//! (ADR-046, PUSH-002/006/007/008/017/018/020).

pub mod events;
pub mod hub;
pub mod protocol;
pub mod replay;
pub mod stream;
pub mod views;
