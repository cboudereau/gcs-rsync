use bytes::Bytes;
use futures::{Stream, StreamExt, TryStreamExt};

use crate::oauth2::token::TokenGenerator;

use super::{
    client::StorageClient,
    resources::object::{ObjectMetadata, Objects},
    Bucket, StorageResult, {Object, ObjectsListRequest, PartialObject},
};

pub struct ObjectClient<T> {
    storage_client: StorageClient<T>,
}

impl<T: TokenGenerator> ObjectClient<T> {
    pub async fn new(token_generator: T) -> StorageResult<Self> {
        Ok(Self {
            storage_client: StorageClient::new(token_generator).await?,
        })
    }
}

impl<T> ObjectClient<T>
where
    T: TokenGenerator,
{
    pub async fn get(&self, o: &Object, fields: &str) -> StorageResult<PartialObject> {
        let url = o.url();
        self.storage_client
            .get_as_json(url.as_str(), &[("fields", fields)])
            .await
    }

    pub async fn delete(&self, o: &Object) -> StorageResult<String> {
        let url = o.url();
        self.storage_client.delete(&url).await?;
        super::StorageResult::Ok(url)
    }

    pub async fn download(
        &self,
        o: &Object,
    ) -> StorageResult<impl Stream<Item = StorageResult<bytes::Bytes>>> {
        let url = o.url();
        Ok(self
            .storage_client
            .get_as_stream(&url, &[("alt", "media")])
            .await?)
    }

    pub async fn upload<S>(&self, o: &Object, stream: S) -> StorageResult<()>
    where
        S: futures::TryStream + Send + Sync + 'static,
        S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
        bytes::Bytes: From<S::Ok>,
    {
        let url = o.upload_url("media");
        self.storage_client.post(&url, stream).await?;
        super::StorageResult::Ok(())
    }

    pub async fn upload_metadata<S, E>(
        &self,
        m: &ObjectMetadata,
        o: &Object,
        stream: S,
        size: u32,
    ) -> StorageResult<()>
    where
        S: futures::Stream<Item = Result<Bytes, E>> + Send + Sync + 'static,
        E: Into<Box<dyn std::error::Error + Send + Sync>> + 'static,
    {
        let url = o.upload_url("multipart");
        self.storage_client
            .post_multipart(&url, m, stream, size)
            .await?;
        super::StorageResult::Ok(())
    }

    pub async fn list(
        &self,
        bucket: &str,
        objects_list_request: &ObjectsListRequest,
    ) -> impl Stream<Item = StorageResult<PartialObject>> + '_ {
        let objects_list_request = objects_list_request.to_owned();
        let url = Bucket::new(bucket).url();
        futures::stream::try_unfold(
            (Some(objects_list_request), url),
            move |(state, url)| async move {
                match state {
                    None => Ok(None),
                    Some(state) => {
                        let objects: Objects =
                            self.storage_client.get_as_json(&url, &state).await?;
                        let items = futures::stream::iter(objects.items).map(Ok);
                        match objects.next_page_token {
                            None => Ok(Some((items, (None, url)))),
                            Some(next_token) => {
                                let new_state = ObjectsListRequest {
                                    page_token: Some(next_token),
                                    ..state
                                };
                                Ok(Some((items, (Some(new_state), url))))
                            }
                        }
                    }
                }
            },
        )
        .try_flatten()
    }
}
