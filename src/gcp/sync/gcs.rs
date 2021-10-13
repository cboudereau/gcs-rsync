use crate::storage::Error as StorageError;
use bytes::Bytes;
use futures::{Stream, StreamExt, TryStreamExt};

use super::{Entry, RSyncError, RelativePath};
use crate::{
    gcp::sync::RSyncResult,
    oauth2::token::TokenGenerator,
    storage::{Object, ObjectClient, ObjectsListRequest, PartialObject},
};

pub(super) struct GcsClient<T> {
    client: ObjectClient<T>,
    object_prefix: ObjectPrefix,
}

#[derive(Clone)]
struct ObjectPrefix {
    bucket: String,
    prefix: String,
    objects_list_request: ObjectsListRequest,
}

impl ObjectPrefix {
    /// Invariant: a prefix should not start by slash but ends with except if empty
    fn new(bucket: &str, prefix: &str) -> Self {
        let bucket = bucket.to_owned();

        let prefix = prefix.strip_prefix('/').unwrap_or(prefix);
        let prefix = if prefix.is_empty() {
            "".to_owned()
        } else if prefix.ends_with('/') {
            prefix.to_owned()
        } else {
            format!("{}/", prefix)
        };

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

    fn as_object(&self, name: &RelativePath) -> Object {
        let name = name.path.as_str();
        if self.prefix.is_empty() {
            return Object::new(&self.bucket, name);
        }

        return Object::new(&self.bucket, format!("{}{}", &self.prefix, name).as_str());
    }

    fn as_relative_path(&self, name: &str) -> RelativePath {
        let prefix = if self.prefix.is_empty() {
            "/"
        } else {
            self.prefix.as_str()
        };
        let path = name.strip_prefix(prefix).unwrap_or(name);
        RelativePath::new(path)
    }
}

impl<T> GcsClient<T>
where
    T: TokenGenerator,
{
    pub(super) async fn new(token_generator: T, bucket: &str, prefix: &str) -> RSyncResult<Self> {
        let object_client = ObjectClient::new(token_generator)
            .await
            .map_err(RSyncError::StorageError)?;
        let object_prefix = ObjectPrefix::new(bucket, prefix);
        Ok(Self {
            client: object_client,
            object_prefix,
        })
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
                        .map(|name| self.object_prefix.as_relative_path(&name))
                        .ok_or_else(|| RSyncError::MissingFieldsInGcsResponse("name".to_owned()))
                })
            })
    }

    pub(super) async fn read(&self, path: &RelativePath) -> impl Stream<Item = RSyncResult<Bytes>> {
        let o = self.object_prefix.as_object(path);
        let download_result = self
            .client
            .download(&o)
            .await
            .map(|x| x.map_err(RSyncError::StorageError))
            .map_err(RSyncError::StorageError);

        futures::stream::once(futures::future::ready(download_result)).try_flatten()
    }

    pub(super) async fn get_crc32c(&self, path: &RelativePath) -> RSyncResult<Option<Entry>> {
        fn to_crc32c(po: PartialObject) -> RSyncResult<u32> {
            po.crc32c
                .map(|x| x.to_u32())
                .ok_or_else(|| RSyncError::MissingFieldsInGcsResponse("crc32c".to_owned()))
        }

        let o = &self.object_prefix.as_object(path);
        let entry = self
            .client
            .get(o, "crc32c")
            .await
            .map_err(RSyncError::StorageError)
            .and_then(to_crc32c)
            .map(|crc32c| Entry::new(path, crc32c));

        match entry {
            Ok(e) => Ok(Some(e)),
            Err(RSyncError::StorageError(StorageError::GcsResourceNotFound)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub(super) async fn exists(&self, path: &RelativePath) -> RSyncResult<bool> {
        let o = &self.object_prefix.as_object(path);
        let entry = self
            .client
            .get(o, "name")
            .await
            .map_err(RSyncError::StorageError);
        match entry {
            Ok(_) => Ok(true),
            Err(RSyncError::StorageError(StorageError::GcsResourceNotFound)) => Ok(false),
            Err(err) => Err(err),
        }
    }

    pub(super) async fn delete(&self, path: &RelativePath) -> RSyncResult<()> {
        let o = self.object_prefix.as_object(path);
        let delete_result = self.client.delete(&o).await;
        match delete_result {
            Ok(_) | Err(StorageError::GcsResourceNotFound) => Ok(()),
            Err(e) => Err(RSyncError::StorageError(e)),
        }
    }

    /// The crc32 comparison is done outside to avoid crc32c calculation when remote is not found
    pub(super) async fn write<S>(&self, path: &RelativePath, stream: S) -> RSyncResult<()>
    where
        S: futures::TryStream<Ok = bytes::Bytes, Error = RSyncError> + Send + Sync + 'static,
    {
        let o = &self.object_prefix.as_object(path);
        self.client
            .upload(o, stream)
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
            Object::new("bucket", "hello"),
            ObjectPrefix::new("bucket", "").as_object(&RelativePath::new("hello"))
        );

        assert_eq!(
            Object::new("bucket", "hello"),
            ObjectPrefix::new("bucket", "").as_object(&RelativePath::new("/hello"))
        );

        assert_eq!(
            Object::new("bucket", "prefix/hello"),
            ObjectPrefix::new("bucket", "prefix").as_object(&RelativePath::new("hello"))
        );

        assert_eq!(
            Object::new("bucket", "prefix/hello/world"),
            ObjectPrefix::new("bucket", "/prefix/hello").as_object(&RelativePath::new("world"))
        );
    }

    #[test]
    fn test_object_prefix() {
        assert_eq!(
            RelativePath::new("hello"),
            ObjectPrefix::new("bucket", "").as_relative_path("hello")
        );

        assert_eq!(
            RelativePath::new("hello"),
            ObjectPrefix::new("bucket", "").as_relative_path("/hello")
        );

        assert_eq!(
            RelativePath::new("hello"),
            ObjectPrefix::new("bucket", "/prefix").as_relative_path("prefix/hello")
        );

        assert_eq!(
            RelativePath::new("hello"),
            ObjectPrefix::new("bucket", "prefix").as_relative_path("prefix/hello")
        );

        assert_eq!(
            RelativePath::new("world"),
            ObjectPrefix::new("bucket", "prefix/hello").as_relative_path("prefix/hello/world")
        );

        assert_eq!(
            RelativePath::new("hello/world"),
            ObjectPrefix::new("bucket", "prefix/").as_relative_path("prefix/hello/world")
        );
    }
}
