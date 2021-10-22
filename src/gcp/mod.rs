pub mod oauth2;
pub mod storage;
pub mod sync;

pub struct Client {
    pub(self) client: reqwest::Client,
}

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
enum DeserializedResponse<T> {
    Success(T),
    Error(serde_json::Value),
}

impl<T> DeserializedResponse<T> {
    pub fn into_result(self) -> Result<T, serde_json::Value> {
        match self {
            DeserializedResponse::Success(x) => Ok(x),
            DeserializedResponse::Error(e) => Err(e),
        }
    }
}

impl Default for Client {
    fn default() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}
