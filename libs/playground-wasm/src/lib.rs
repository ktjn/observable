//! Phase 0 viability spike only: proves a Rust function compiles to
//! `wasm32-unknown-unknown` and can be called from JS via `wasm-bindgen`.
//! Not connected to any real Observable domain logic yet.

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn ping(seed: u32) -> String {
    format!("observable-playground-wasm:{seed}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_includes_seed() {
        assert_eq!(ping(42), "observable-playground-wasm:42");
    }
}
