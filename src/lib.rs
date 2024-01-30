pub mod da;
pub mod error;
mod utils;
use da::{CelestiaConnection, DataAvailabilityLayer};
use std::sync::Arc;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;

#[wasm_bindgen]
extern "C" {
    fn alert(s: &str);
}

#[wasm_bindgen]
pub fn greet() {
    alert("Hello, deimos-wasm!");
}

async fn initialize_da_layer() -> Option<Arc<dyn DataAvailabilityLayer + 'static>> {
    match CelestiaConnection::new(
        &"ws://localhost:26658".to_string(),
        &"00000000000000de1008".to_string(),
    )
    .await
    {
        Ok(da) => Some(Arc::new(da) as Arc<dyn DataAvailabilityLayer + 'static>),
        Err(e) => {
            alert("Failed to connect to Celestia");
            None
        }
    }
}

#[wasm_bindgen]
pub fn start_lightclient() {
    spawn_local(async {
        let lightclient = initialize_da_layer().await.unwrap();
        lightclient.start().await;
    });
}
