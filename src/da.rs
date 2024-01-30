use std::convert::TryFrom;
use std::sync::{Arc, RwLock};

use crate::error::GeneralError;
use async_trait::async_trait;
use celestia_rpc::{client::WasmClient, BlobClient, HeaderClient};
use celestia_types::{nmt::Namespace, Blob};
use serde::{Deserialize, Serialize};

use crate::error::DataAvailabilityError;

// TODO: Add signature from sequencer for lc to verify (#2)
#[derive(Serialize, Deserialize)]
pub struct EpochJson {
    pub height: u64,
    pub prev_commitment: String,
    pub current_commitment: String,
}

impl TryFrom<&Blob> for EpochJson {
    type Error = GeneralError;

    fn try_from(value: &Blob) -> Result<Self, GeneralError> {
        // convert blob data to utf8 string
        let data_str = String::from_utf8(value.data.clone()).map_err(|e| {
            GeneralError::ParsingError(format!("Could not convert blob data to utf8 string: {}", e))
        })?;

        // convert utf8 string to EpochJson
        serde_json::from_str(&data_str)
            .map_err(|e| GeneralError::ParsingError(format!("Could not parse epoch json: {}", e)))
    }
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Deserialize)]
#[cfg_attr(feature = "serde", derive(SerializeDisplay, DeserializeFromStr))]
enum DALayerOption {
    #[default]
    Celestia,
    #[cfg(test)]
    InMemory,
    None,
}

#[async_trait]
pub trait DataAvailabilityLayer: Send + Sync {
    async fn get_message(&self) -> Result<u64, DataAvailabilityError>;
    async fn initialize_sync_target(&self) -> Result<u64, DataAvailabilityError>;
    async fn get(&self, height: u64) -> Result<Vec<EpochJson>, DataAvailabilityError>;
    async fn start(&self) -> Result<(), DataAvailabilityError>;
}

pub struct CelestiaConnection {
    pub client: WasmClient,
    pub namespace_id: Namespace,
    sync_target: Arc<RwLock<u64>>,
}

impl CelestiaConnection {
    // TODO: Should take config
    pub async fn new(
        connection_string: &String,
        namespace_hex: &String,
    ) -> Result<Self, DataAvailabilityError> {
        let client = WasmClient::new(&connection_string).await.map_err(|e| {
            DataAvailabilityError::InitializationError(format!(
                "Websocket initialization failed: {}",
                e
            ))
        })?;

        let decoded_hex = match hex::decode(namespace_hex) {
            Ok(hex) => hex,
            Err(e) => {
                return Err(DataAvailabilityError::InitializationError(format!(
                    "Hex decoding failed: {}",
                    e
                )))
            }
        };

        let namespace_id = Namespace::new_v0(&decoded_hex).map_err(|e| {
            DataAvailabilityError::InitializationError(format!("Namespace creation failed: {}", e))
        })?;

        Ok(CelestiaConnection {
            client,
            namespace_id,
            sync_target: Arc::new(RwLock::new(0)),
        })
    }
}

#[async_trait]
impl DataAvailabilityLayer for CelestiaConnection {
    async fn get_message(&self) -> Result<u64, DataAvailabilityError> {
        let data = self.sync_target.read().map_err(|_| {
            DataAvailabilityError::SyncTargetError(
                "reading".to_string(),
                "Could not read sync target".to_string(),
            )
        })?;
        Ok(*data)
    }

    async fn initialize_sync_target(&self) -> Result<u64, DataAvailabilityError> {
        match HeaderClient::header_network_head(&self.client).await {
            Ok(extended_header) => Ok(extended_header.header.height.value()),
            Err(err) => Err(DataAvailabilityError::NetworkError(format!(
                "Could not get network head from DA layer: {}",
                err
            ))),
        }
    }

    async fn get(&self, height: u64) -> Result<Vec<EpochJson>, DataAvailabilityError> {
        println! {"Getting epoch {} from DA layer", height};
        match BlobClient::blob_get_all(&self.client, height, &[self.namespace_id]).await {
            Ok(blobs) => {
                let mut epochs = Vec::new();
                for blob in blobs.iter() {
                    match EpochJson::try_from(blob) {
                        Ok(epoch_json) => epochs.push(epoch_json),
                        Err(_) => {
                            DataAvailabilityError::DataRetrievalError(
                                height,
                                "Could not parse epoch json for blob".to_string(),
                            );
                        }
                    }
                }
                Ok(epochs)
            }
            Err(err) => Err(DataAvailabilityError::DataRetrievalError(
                height,
                format!("Could not get epoch from DA layer: {}", err),
            )),
        }
    }

    async fn start(&self) -> Result<(), DataAvailabilityError> {
        println!("Starting light client");

        let mut header_sub = HeaderClient::header_subscribe(&self.client)
            .await
            .map_err(|e| {
                DataAvailabilityError::NetworkError(format!(
                    "Could not subscribe to header updates from DA layer: {}",
                    e
                ))
            })?;

        let sync_target_mutex_copy = self.sync_target.clone();
        wasm_bindgen_futures::spawn_local(async move {
            while let Some(extended_header_result) = header_sub.next().await {
                match extended_header_result {
                    Ok(extended_header) => {
                        let mut data = sync_target_mutex_copy.write().unwrap();
                        *data = extended_header.header.height.value();
                    }
                    Err(_) => {
                        DataAvailabilityError::NetworkError(
                            "Could not get header from DA layer".to_string(),
                        );
                    }
                }
            }
        });
        Ok(())
    }
}
