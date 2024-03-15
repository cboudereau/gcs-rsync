use crate::storage::{Error as StorageError, Metadata, ObjectMetadata};
use bytes::Bytes;
use chrono::TimeZone;
use futures::{Stream, StreamExt, TryStreamExt};

use super::{Entry, RSyncError, RelativePath};
use crate::{
    gcp::sync::RSyncResult,
    oauth2::token::TokenGenerator,
    storage::{Object, ObjectClient, ObjectsListRequest, PartialObject},
};

pub(super) struct GcsClient {
    client: ObjectClient,
    object_prefix: ObjectPrefix,
}

#[derive(Clone)]
struct ObjectPrefix {
    bucket: String,
    prefix: String,
    objects_list_request: ObjectsListRequest,
}

impl ObjectPrefix {
    fn new(bucket: &str, prefix: &str) -> Self {
        let bucket = bucket.to_owned();
        let prefix = prefix.to_owned();

        let objects_list_request = ObjectsListRequest {
            prefix: Some(prefix.to_owned()),
            fields: Some("items(name),nextPageToken".to_owned()),
            ..Default::default()
        };

        Self {
            bucket,
            prefix,
            objects_list_request,
        }
    }

    fn as_object(&self, name: &RelativePath) -> RSyncResult<Object> {
        let name = name.path.as_str();

        let object = Object::new(&self.bucket, name);
        object.map_err(RSyncError::StorageError)
    }

    fn as_relative_path(&self, name: &str) -> RSyncResult<RelativePath> {
        let prefix = if self.prefix.is_empty() {
            "/"
        } else {
            self.prefix.as_str()
        };
        let path = if self.prefix.ends_with("/") {
            name.strip_prefix(prefix).unwrap_or(name)
        } else {
            name
        };
        RelativePath::new(path)
    }
}

type Size = u64;

impl GcsClient {
    pub(super) async fn new(
        token_generator: Box<dyn TokenGenerator>,
        bucket: &str,
        prefix: &str,
    ) -> RSyncResult<Self> {
        let object_client = ObjectClient::new(token_generator)
            .await
            .map_err(RSyncError::StorageError)?;
        let object_prefix = ObjectPrefix::new(bucket, prefix);
        Ok(Self {
            client: object_client,
            object_prefix,
        })
    }

    pub(super) fn no_auth(bucket: &str, prefix: &str) -> Self {
        let object_client = ObjectClient::no_auth();
        let object_prefix = ObjectPrefix::new(bucket, prefix);
        Self {
            client: object_client,
            object_prefix,
        }
    }

    pub(super) async fn list(&self) -> impl Stream<Item = RSyncResult<RelativePath>> + '_ {
        self.client
            .list(
                &self.object_prefix.bucket,
                &self.object_prefix.objects_list_request,
            )
            .await
            .map_err(RSyncError::StorageError)
            .map(move |r| {
                r.and_then(|po| {
                    po.name
                        .ok_or_else(|| RSyncError::MissingFieldsInGcsResponse("name".to_owned()))
                        .and_then(|name| self.object_prefix.as_relative_path(&name))
                })
            })
    }

    pub(super) async fn read(&self, path: &RelativePath) -> impl Stream<Item = RSyncResult<Bytes>> {
        let download_result = async {
            let o = self.object_prefix.as_object(path)?;
            self.client
                .download(&o)
                .await
                .map(|x| x.map_err(RSyncError::StorageError))
                .map_err(RSyncError::StorageError)
        }
        .await;

        futures::stream::once(futures::future::ready(download_result)).try_flatten()
    }

    pub(super) async fn get_crc32c(&self, path: &RelativePath) -> RSyncResult<Option<Entry>> {
        fn to_crc32c(po: PartialObject) -> RSyncResult<u32> {
            po.crc32c
                .map(|x| x.to_u32())
                .ok_or_else(|| RSyncError::MissingFieldsInGcsResponse("crc32c".to_owned()))
        }

        let o = &self.object_prefix.as_object(path)?;
        let entry = self
            .client
            .get(o, "crc32c")
            .await
            .map_err(RSyncError::StorageError)
            .and_then(to_crc32c)
            .map(|crc32c| Entry::new(path, crc32c));

        match entry {
            Ok(e) => Ok(Some(e)),
            Err(RSyncError::StorageError(StorageError::GcsResourceNotFound { .. })) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub(super) async fn exists(&self, path: &RelativePath) -> RSyncResult<bool> {
        let o = &self.object_prefix.as_object(path)?;
        let entry = self
            .client
            .get(o, "name")
            .await
            .map_err(RSyncError::StorageError);
        match entry {
            Ok(_) => Ok(true),
            Err(RSyncError::StorageError(StorageError::GcsResourceNotFound { .. })) => Ok(false),
            Err(err) => Err(err),
        }
    }

    pub(super) async fn size_and_mt(
        &self,
        path: &RelativePath,
    ) -> RSyncResult<(Option<chrono::DateTime<chrono::Utc>>, Option<Size>)> {
        let o = &self.object_prefix.as_object(path)?;
        let entry = self
            .client
            .get(o, "size,metadata/goog-reserved-file-mtime")
            .await
            .map_err(RSyncError::StorageError);

        match entry {
            Ok(entry) => {
                let size = entry.size;
                let date_time = entry
                    .metadata
                    .and_then(|x| x.modification_time)
                    .and_then(|mtime| chrono::offset::Utc.timestamp_opt(mtime, 0).single());
                Ok((date_time, size))
            }
            Err(RSyncError::StorageError(StorageError::GcsResourceNotFound { .. })) => {
                Ok((None, None))
            }
            Err(err) => Err(err),
        }
    }

    pub(super) async fn delete(&self, path: &RelativePath) -> RSyncResult<()> {
        let o = self.object_prefix.as_object(path)?;
        let delete_result = self.client.delete(&o).await;
        match delete_result {
            Ok(_) | Err(StorageError::GcsResourceNotFound { .. }) => Ok(()),
            Err(e) => Err(RSyncError::StorageError(e)),
        }
    }

    /// The crc32 comparison is done outside to avoid crc32c calculation when remote is not found
    pub(super) async fn write<S>(&self, path: &RelativePath, stream: S) -> RSyncResult<()>
    where
        S: futures::TryStream<Ok = bytes::Bytes, Error = RSyncError> + Send + Sync + 'static,
    {
        let o = &self.object_prefix.as_object(path)?;
        self.client
            .upload(o, stream)
            .await
            .map_err(RSyncError::StorageError)
            .map(|_| ())
    }

    pub(super) async fn write_mtime<S>(
        &self,
        mtime: chrono::DateTime<chrono::Utc>,
        path: &RelativePath,
        stream: S,
    ) -> RSyncResult<()>
    where
        S: futures::TryStream<Ok = bytes::Bytes, Error = RSyncError> + Send + Sync + 'static,
    {
        let o = &self.object_prefix.as_object(path)?;
        let mtime = mtime.timestamp();
        let m = ObjectMetadata {
            metadata: {
                Metadata {
                    modification_time: Some(mtime),
                }
            },
        };
        self.client
            .upload_with_metadata(&m, o, stream)
            .await
            .map_err(RSyncError::StorageError)
            .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use crate::{gcp::sync::RelativePath, storage::Object};

    use super::ObjectPrefix;

    #[test]
    fn test_object_prefix_as_object() {
        assert_eq!(
            Object::new("bucket", "hello").unwrap(),
            ObjectPrefix::new("bucket", "")
                .as_object(&RelativePath::new("hello").unwrap())
                .unwrap()
        );

        assert_eq!(
            Object::new("bucket", "hello").unwrap(),
            ObjectPrefix::new("bucket", "")
                .as_object(&RelativePath::new("/hello").unwrap())
                .unwrap()
        );

        assert_eq!(
            Object::new("bucket", "prefix/hello").unwrap(),
            ObjectPrefix::new("bucket", "prefix")
                .as_object(&RelativePath::new("hello").unwrap())
                .unwrap()
        );

        assert_eq!(
            Object::new("bucket", "prefix/hello/world").unwrap(),
            ObjectPrefix::new("bucket", "/prefix/hello")
                .as_object(&RelativePath::new("world").unwrap())
                .unwrap()
        );
    }

    #[test]
    fn test_object_prefix() {
        assert_eq!(
            RelativePath::new("hello").unwrap(),
            ObjectPrefix::new("bucket", "")
                .as_relative_path("hello")
                .unwrap()
        );

        assert_eq!(
            RelativePath::new("hello").unwrap(),
            ObjectPrefix::new("bucket", "")
                .as_relative_path("/hello")
                .unwrap()
        );

        assert_eq!(
            RelativePath::new("hello").unwrap(),
            ObjectPrefix::new("bucket", "/prefix")
                .as_relative_path("prefix/hello")
                .unwrap()
        );

        assert_eq!(
            RelativePath::new("hello").unwrap(),
            ObjectPrefix::new("bucket", "prefix")
                .as_relative_path("prefix/hello")
                .unwrap()
        );

        assert_eq!(
            RelativePath::new("world").unwrap(),
            ObjectPrefix::new("bucket", "prefix/hello")
                .as_relative_path("prefix/hello/world")
                .unwrap()
        );

        assert_eq!(
            RelativePath::new("hello/world").unwrap(),
            ObjectPrefix::new("bucket", "prefix/")
                .as_relative_path("prefix/hello/world")
                .unwrap()
        );
    }
}
