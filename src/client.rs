use jsonrpsee_wasm_client::WasmClientBuilder;
pub use jsonrpsee_core::client::{Client as WsClient, Error as WsError};
use serde::{Serialize, Deserialize};
use celestia_types::{Result as CelestiaResult, ExtendedHeader};
use serde_json::json;

pub type Result<T> = std::result::Result<T, Error>;


#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Protocol specified in connection string is not supported.
    #[error("Protocol not supported or missing: {0}")]
    ProtocolNotSupported(String),

    /// Error from the underlying transport.
    #[error("Transport error: {0}")]
    TransportError(#[from] WsError),
}

pub enum Client {
    Ws(WsClient),
}

impl Client {
    /// Create a new Json RPC client.
    ///
    /// Only ws(s) protocol is supported in wasm environment.
    pub async fn new(conn_str: &str, auth_token: Option<&str>) -> Result<Self> {
        // Since headers are not supported in the current version of `jsonrpsee-wasm-client`,
        // we cannot set the authorization token directly in the header.
        // we might need to pass the token in a different way, e.g., as part of the RPC calls.
        // update: we wont need to pass the token in the RPC calls, since ryan will add a feature to do it without token

        let protocol = conn_str.split_once(':').map(|(proto, _)| proto);
        let client = match protocol {
            Some("ws") | Some("wss") => match WasmClientBuilder::default().build(conn_str).await {
                Ok(client) => Client::Ws(client),
                Err(e) => return Err(Error::TransportError(e)),
            },
            _ => return Err(Error::ProtocolNotSupported(conn_str.into())),
        };

        Ok(client)
    }


    pub async fn header_subscribe(&self) -> CelestiaResult<(), Error> {
        match &self {
            Client::Ws(ws_client) => {
                let onmessage_callback = Closure::wrap(Box::new(move |e: MessageEvent| {
                    if let Ok(txt) = e.data().dyn_into::<js_sys::JsString>() {
                        // ? something like this
                        println!("Message from server: {:?}", txt);
                    }
                }) as Box<dyn FnMut(MessageEvent)>);

                ws_client.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
                onmessage_callback.forget();

                // do we need to send a request on our own to subscribe?
                let rpc_request = json!({
                    "jsonrpc": "2.0",
                    "method": "header.Subscribe",
                    "params": [], 
                    "id": 1
                }).to_string();

                ws_client.send_with_str(&rpc_request)?;

                Ok(())
            }
        }
    }
}
