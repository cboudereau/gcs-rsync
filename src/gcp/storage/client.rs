use super::{Error, StorageResult};
use crate::gcp::{
    oauth2::token::{AccessToken, Token, TokenGenerator},
    Client,
};
use futures::{Stream, TryStream, TryStreamExt};
use serde::{de::DeserializeOwned, Serialize};
use tokio::sync::RwLock;

#[derive(Debug)]
pub(super) struct StorageClient<T> {
    client: Client,
    token_generator: T,
    token: RwLock<Token>,
}

impl<T: TokenGenerator> StorageClient<T> {
    pub async fn new(token_generator: T) -> StorageResult<Self> {
        let client = Client::default();
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

    async fn refresh_token(&self) -> StorageResult<AccessToken> {
        let t = self.token.read().await;
        if t.is_valid() {
            Ok(t.access_token())
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

    async fn success_response(response: reqwest::Response) -> StorageResult<reqwest::Response> {
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }

        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(super::Error::GcsResourceNotFound);
        }

        let err = response.json().await.map_err(super::Error::GcsHttpError)?;
        Err(super::Error::GcsUnexpectedResponse(err))
    }

    pub async fn delete(&self, url: &str) -> StorageResult<()> {
        let response = self
            .client
            .client
            .delete(url)
            .bearer_auth(self.refresh_token().await?)
            .send()
            .await
            .map_err(super::Error::GcsHttpError)?;
        Self::success_response(response).await?;
        Ok(())
    }

    pub async fn post<S>(&self, url: &str, body: S) -> StorageResult<()>
    where
        S: TryStream + Send + Sync + 'static,
        S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
        bytes::Bytes: From<S::Ok>,
    {
        let response = self
            .client
            .client
            .post(url)
            .bearer_auth(self.refresh_token().await?)
            .body(reqwest::Body::wrap_stream(body))
            .send()
            .await
            .map_err(super::Error::GcsHttpError)?;

        Self::success_response(response).await?;
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
        let response = self
            .client
            .client
            .get(url)
            .bearer_auth(self.refresh_token().await?)
            .query(query)
            .send()
            .await
            .map_err(super::Error::GcsHttpError)?;

        Ok(Self::success_response(response)
            .await?
            .bytes_stream()
            .map_err(super::Error::GcsHttpError))
    }

    pub async fn get_as_json<R, Q>(&self, url: &str, query: &Q) -> StorageResult<R>
    where
        R: DeserializeOwned,
        Q: serde::Serialize,
    {
        let response = self
            .client
            .client
            .get(url)
            .query(query)
            .bearer_auth(self.refresh_token().await?)
            .send()
            .await
            .map_err(super::Error::GcsHttpError)?;
        let r: super::super::DeserializedResponse<R> = Self::success_response(response)
            .await?
            .json()
            .await
            .map_err(super::Error::GcsHttpError)?;
        r.into_result().map_err(super::Error::GcsUnexpectedResponse)
    }
}
