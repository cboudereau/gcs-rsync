use super::{Error, StorageResult};
use crate::gcp::{
    oauth2::token::{AccessToken, Token, TokenGenerator},
    Client,
};
use bytes::BufMut;
use futures::{
    stream,
    stream::{Stream, StreamExt, TryStream, TryStreamExt},
};
use reqwest::RequestBuilder;
use serde::{de::DeserializeOwned, Serialize};
use tokio::sync::RwLock;

#[derive(Debug)]
struct TokenStateHolder {
    client: Client,
    token_generator: Box<dyn TokenGenerator>,
    token: RwLock<Token>,
}

impl TokenStateHolder {
    pub async fn new(
        client: Client,
        token_generator: Box<dyn TokenGenerator>,
    ) -> StorageResult<Self> {
        let token = token_generator
            .get(&client)
            .await
            .map_err(Error::GcsTokenError)?;
        Ok(Self {
            client,
            token_generator,
            token: RwLock::new(token),
        })
    }

    async fn get_token(&self) -> Option<AccessToken> {
        let t = self.token.read().await;

        if t.is_valid() {
            Some(t.access_token())
        } else {
            None
        }
    }

    async fn refresh_token(&self) -> StorageResult<AccessToken> {
        if let Some(token) = self.get_token().await {
            Ok(token)
        } else {
            let t = self
                .token_generator
                .get(&self.client)
                .await
                .map_err(Error::GcsTokenError)?;
            let access_token = t.access_token();
            *self.token.write().await = t;
            Ok(access_token)
        }
    }
}

#[derive(Debug)]
pub(super) struct StorageClient {
    client: Client,
    token_state_holder: Option<TokenStateHolder>,
}

const MT_SEPARATOR: &[u8] = b"--gcs-storage\n";
const MT_END_SEPARATOR: &[u8] = b"\n--gcs-storage--";
const MT_CONTENT_TYPE: &[u8] = b"Content-Type: application/octet-stream";
const MT_METADATA_TYPE: &[u8] = b"Content-Type: application/json; charset=utf-8\n\n";

impl StorageClient {
    pub async fn new(token_generator: Box<dyn TokenGenerator>) -> StorageResult<Self> {
        let client = Client::default();
        let token_state_holder =
            Some(TokenStateHolder::new(client.clone(), token_generator).await?);

        Ok(Self {
            client,
            token_state_holder,
        })
    }

    pub fn no_auth() -> Self {
        let client = Client::default();
        let token_state_holder = None;

        Self {
            client,
            token_state_holder,
        }
    }

    async fn success_response(
        url: &str,
        response: reqwest::Response,
    ) -> StorageResult<reqwest::Response> {
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }

        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(super::Error::GcsResourceNotFound {
                url: url.to_owned(),
            });
        }

        let err = response.text().await.map_err(super::Error::GcsHttpError)?;
        Err(super::Error::gcs_unexpected_response_error(url, err))
    }

    async fn with_auth(&self, request_builder: RequestBuilder) -> StorageResult<RequestBuilder> {
        match &self.token_state_holder {
            None => Ok(request_builder),
            Some(token_state_holder) => {
                Ok(request_builder.bearer_auth(token_state_holder.refresh_token().await?))
            }
        }
    }

    pub async fn delete(&self, url: &str) -> StorageResult<()> {
        let request = self.with_auth(self.client.client.delete(url)).await?;
        let response = request.send().await.map_err(super::Error::GcsHttpError)?;
        Self::success_response(url, response).await?;
        Ok(())
    }

    pub async fn post<S>(&self, url: &str, body: S) -> StorageResult<()>
    where
        S: TryStream + Send + Sync + 'static,
        S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
        bytes::Bytes: From<S::Ok>,
    {
        let request = self.with_auth(self.client.client.post(url)).await?;
        let response = request
            .body(reqwest::Body::wrap_stream(body))
            .send()
            .await
            .map_err(super::Error::GcsHttpError)?;

        Self::success_response(url, response).await?;
        Ok(())
    }

    // Specs: https://cloud.google.com/storage/docs/json_api/v1/how-tos/multipart-upload
    // POST https://www.googleapis.com/upload/storage/v1/b/test-bucket/o?uploadType=multipart&name=path%2Fobject.txt HTTP/1.1
    // Authorization: Bearer <Token>
    // Content-Type: multipart/related; boundary=gcs-storage
    // Content-Length: <BodyLength>
    //
    // --gcs-storage
    // Content-Type: application/json; charset=UTF-8
    //
    // {
    // "test": "hello"
    // }
    //
    // --gcs-storage
    // Content-Type: application/octet-stream
    //
    // <ObjectStream>
    // --gcs-storage--
    pub async fn post_multipart<S, M>(&self, url: &str, metadata: &M, body: S) -> StorageResult<()>
    where
        M: Serialize,
        S: TryStream<Ok = bytes::Bytes> + Send + Sync + 'static,
        S::Error: Into<Box<dyn std::error::Error + Send + Sync>> + Send + Sync,
    {
        let json = serde_json::ser::to_vec(metadata).map_err(Error::gcs_invalid_metadata::<M>)?;

        let part_len = 2 * MT_SEPARATOR.len()
            + MT_METADATA_TYPE.len()
            + json.len()
            + MT_CONTENT_TYPE.len()
            + 3;
        let part = {
            let mut part = bytes::BytesMut::with_capacity(part_len);
            part.put_slice(MT_SEPARATOR);
            part.put_slice(MT_METADATA_TYPE);
            part.put_slice(&json);
            part.put_slice(b"\n");
            part.put_slice(MT_SEPARATOR);
            part.put_slice(MT_CONTENT_TYPE);
            part.put_slice(b"\n\n");

            part.freeze()
        };
        let mbody = {
            stream::iter([Ok(part)])
                .chain(body.into_stream())
                .chain(stream::iter([Ok(bytes::Bytes::from_static(
                    MT_END_SEPARATOR,
                ))]))
        };

        // let total_len = part_len as u64 + size + MT_END_SEPARATOR.len() as u64;
        let request = self.client.client.post(url);
        let request = self.with_auth(request).await?;
        let response = request
            .header("Content-Type", "multipart/related; boundary=gcs-storage")
            // .header("Content-Length", total_len)
            .body(reqwest::Body::wrap_stream(mbody))
            .send()
            .await
            .map_err(super::Error::GcsHttpError)?;

        Self::success_response(url, response).await?;
        Ok(())
    }

    pub async fn get_as_stream<Q>(
        &self,
        url: &str,
        query: &Q,
    ) -> StorageResult<impl Stream<Item = StorageResult<bytes::Bytes>>>
    where
        Q: Serialize,
    {
        let request = self.with_auth(self.client.client.get(url)).await?;
        let response = request
            .query(query)
            .send()
            .await
            .map_err(super::Error::GcsHttpError)?;

        Ok(Self::success_response(url, response)
            .await?
            .bytes_stream()
            .map_err(super::Error::GcsHttpError))
    }

    pub async fn get_as_json<R, Q>(&self, url: &str, query: &Q) -> StorageResult<R>
    where
        R: DeserializeOwned,
        Q: serde::Serialize,
    {
        let request = self
            .with_auth(self.client.client.get(url).query(query))
            .await?;
        let response = request.send().await.map_err(super::Error::GcsHttpError)?;
        let r: super::super::DeserializedResponse<R> = Self::success_response(url, response)
            .await?
            .json()
            .await
            .map_err(super::Error::GcsHttpError)?;
        r.into_result()
            .map_err(|err| super::Error::gcs_unexpected_json::<R>(url, err))
    }
}
