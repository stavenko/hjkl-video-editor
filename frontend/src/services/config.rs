use std::sync::OnceLock;

use serde::Deserialize;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, RequestInit, RequestMode, Response};

const CONFIG_URL: &str = "/config/frontend.toml";

#[derive(Debug, Clone, Deserialize)]
pub struct FrontendConfig {
    pub api_base_url: String,
}

static CONFIG: OnceLock<FrontendConfig> = OnceLock::new();

pub fn get() -> &'static FrontendConfig {
    CONFIG
        .get()
        .expect("Frontend config not initialized — call services::config::load() before mount")
}

pub async fn load() {
    let config = fetch_config().await;
    CONFIG
        .set(config)
        .expect("Frontend config already initialized");
}

async fn fetch_config() -> FrontendConfig {
    let opts = RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(RequestMode::Cors);

    let request = Request::new_with_str_and_init(CONFIG_URL, &opts)
        .unwrap_or_else(|e| panic!("Failed to build config request: {:?}", e));

    let window = web_sys::window().expect("window not available");
    let response_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .unwrap_or_else(|e| panic!("Failed to fetch {}: {:?}", CONFIG_URL, e));
    let response: Response = response_value
        .dyn_into()
        .unwrap_or_else(|_| panic!("Response cast failed for {}", CONFIG_URL));

    if !response.ok() {
        panic!(
            "Failed to load {}: HTTP {}",
            CONFIG_URL,
            response.status()
        );
    }

    let text_promise = response
        .text()
        .unwrap_or_else(|e| panic!("Failed to read {} body: {:?}", CONFIG_URL, e));
    let text_value = JsFuture::from(text_promise)
        .await
        .unwrap_or_else(|e| panic!("Failed to await {} text: {:?}", CONFIG_URL, e));
    let text = text_value
        .as_string()
        .unwrap_or_else(|| panic!("{} body is not a string", CONFIG_URL));

    toml::from_str::<FrontendConfig>(&text)
        .unwrap_or_else(|e| panic!("Failed to parse {}: {}", CONFIG_URL, e))
}
