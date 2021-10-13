mod fs;
mod gcs;

use std::ops::Not;
use std::path::Path;

use bytes::Bytes;
use futures::future::Either;
use futures::{Future, Stream, StreamExt, TryStreamExt};

use fs::FsClient;
use gcs::GcsClient;

pub struct ReaderWriter<T> {
    inner: ReaderWriterInternal<T>,
}

pub type DefaultSource = ReaderWriter<crate::oauth2::token::AuthorizedUserCredentials>;
pub type GcsSource<T> = ReaderWriter<T>;

impl<T> ReaderWriter<T>
where
    T: crate::oauth2::token::TokenGenerator,
{
    fn new(inner: ReaderWriterInternal<T>) -> Self {
        Self { inner }
    }

    pub async fn gcs(token_generator: T, bucket: &str, prefix: &str) -> RSyncResult<Self> {
        let client = GcsClient::new(token_generator, bucket, prefix).await?;
        Ok(Self::new(ReaderWriterInternal::Gcs(client)))
    }

    pub fn fs(base_path: &Path) -> Self {
        let client = FsClient::new(base_path);
        Self::new(ReaderWriterInternal::Fs(client))
    }
}

//TODO: replace this with trait when async trait will be more stable with method returning Trait
enum ReaderWriterInternal<T> {
    Gcs(GcsClient<T>),
    Fs(FsClient),
}

impl<T> ReaderWriterInternal<T>
where
    T: crate::oauth2::token::TokenGenerator,
{
    async fn list(
        &self,
    ) -> Either<
        impl Stream<Item = RSyncResult<RelativePath>> + '_,
        impl Stream<Item = RSyncResult<RelativePath>> + '_,
    > {
        match self {
            ReaderWriterInternal::Gcs(client) => Either::Left(client.list().await),
            ReaderWriterInternal::Fs(client) => Either::Right(client.list().await),
        }
    }

    async fn read(
        &self,
        path: &RelativePath,
    ) -> Either<impl Stream<Item = RSyncResult<Bytes>>, impl Stream<Item = RSyncResult<Bytes>>>
    {
        match self {
            ReaderWriterInternal::Gcs(client) => Either::Left(client.read(path).await),
            ReaderWriterInternal::Fs(client) => Either::Right(client.read(path).await),
        }
    }

    async fn get_crc32c(&self, path: &RelativePath) -> RSyncResult<Option<Entry>> {
        match self {
            ReaderWriterInternal::Gcs(client) => client.get_crc32c(path).await,
            ReaderWriterInternal::Fs(client) => client.get_crc32c(path).await,
        }
    }

    async fn write<S>(&self, path: &RelativePath, stream: S) -> RSyncResult<()>
    where
        S: futures::TryStream<Ok = bytes::Bytes, Error = RSyncError>
            + Send
            + Sync
            + std::marker::Unpin
            + 'static,
    {
        match self {
            ReaderWriterInternal::Gcs(client) => client.write(path, stream).await,
            ReaderWriterInternal::Fs(client) => client.write(path, stream).await,
        }
    }

    async fn delete(&self, path: &RelativePath) -> RSyncResult<()> {
        match self {
            ReaderWriterInternal::Gcs(client) => client.delete(path).await,
            ReaderWriterInternal::Fs(client) => client.delete(path).await,
        }
    }

    async fn exists(&self, path: &RelativePath) -> RSyncResult<bool> {
        match self {
            ReaderWriterInternal::Gcs(client) => client.exists(path).await,
            ReaderWriterInternal::Fs(client) => client.exists(path).await,
        }
    }
}

pub type FsRSync = RSync<crate::oauth2::token::AuthorizedUserCredentials>;
pub struct RSync<T> {
    source: ReaderWriterInternal<T>,
    dest: ReaderWriterInternal<T>,
}

impl<T> RSync<T>
where
    T: crate::oauth2::token::TokenGenerator + 'static,
{
    pub fn new(source: ReaderWriter<T>, dest: ReaderWriter<T>) -> Self {
        Self {
            source: source.inner,
            dest: dest.inner,
        }
    }

    async fn write_entry(&self, path: &RelativePath) -> RSyncResult<()> {
        let source = self.source.read(path).await;
        self.dest.write(path, source).await?;
        Ok(())
    }

    async fn sync_entry(&self, path: &RelativePath) -> RSyncResult<RSyncStatus> {
        let crc = self.dest.get_crc32c(path).await?;

        Ok(match crc {
            None => {
                self.write_entry(path).await?;
                RSyncStatus::Created(path.to_owned())
            }
            Some(crc32c_dest) => {
                let crc32c_source = self.source.get_crc32c(path).await?;
                if Some(crc32c_dest) == crc32c_source {
                    RSyncStatus::AlreadySynced(path.to_owned())
                } else {
                    self.write_entry(path).await?;
                    RSyncStatus::Updated(path.to_owned())
                }
            }
        })
    }

    pub async fn sync(
        &self,
    ) -> impl Stream<Item = RSyncResult<impl Future<Output = RSyncResult<RSyncStatus>> + '_>> + '_
    {
        self.source
            .list()
            .await
            .map_ok(move |path| async move { self.sync_entry(&path).await })
    }

    async fn delete_extras(
        &self,
    ) -> impl Stream<Item = RSyncResult<impl Future<Output = RSyncResult<RMirrorStatus>> + '_>> + '_
    {
        let r = self.dest.list().await.map(move |result| {
            result.map(|path| async move {
                if self.source.exists(&path).await?.not() {
                    self.dest.delete(&path).await?;
                    Ok(RMirrorStatus::Deleted(path))
                } else {
                    Ok(RMirrorStatus::NotDeleted(path))
                }
            })
        });

        r
    }

    pub async fn mirror(
        &self,
    ) -> impl Stream<Item = RSyncResult<impl Future<Output = RSyncResult<RMirrorStatus>> + '_>> + '_
    {
        let synced = self
            .sync()
            .await
            .map_ok(|fut| async { fut.await.map(RMirrorStatus::Synced) })
            .map_ok(futures::future::Either::Left);

        let deleted = self
            .delete_extras()
            .await
            .map_ok(futures::future::Either::Right);

        synced.chain(deleted)
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RelativePath {
    path: String,
}

impl std::fmt::Debug for RelativePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "{}", self.path)
    }
}

impl RelativePath {
    /// Invariant: a name should not start with a slash
    pub fn new(path: &str) -> Self {
        let path = path.strip_prefix('/').unwrap_or(path).to_owned();
        Self { path }
    }
}
#[derive(Debug, PartialEq, Clone)]
struct Entry {
    path: RelativePath,
    crc32c: u32,
}

impl Entry {
    pub(self) fn new(path: &RelativePath, crc32c: u32) -> Self {
        Self {
            path: path.to_owned(),
            crc32c,
        }
    }
}

#[derive(Debug)]
pub enum RSyncError {
    MissingFieldsInGcsResponse(String),
    StorageError(super::storage::Error),
    FsIoError(std::io::Error),
}

impl std::fmt::Display for RSyncError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::error::Error for RSyncError {}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RSyncStatus {
    Created(RelativePath),
    Updated(RelativePath),
    AlreadySynced(RelativePath),
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RMirrorStatus {
    Synced(RSyncStatus),
    Deleted(RelativePath),
    NotDeleted(RelativePath),
}

pub type RSyncResult<T> = Result<T, RSyncError>;

#[cfg(test)]
mod tests {
    use crate::gcp::sync::RelativePath;

    #[test]
    fn test_relative_path() {
        assert_eq!("", RelativePath::new("").path);
        assert_eq!("", RelativePath::new("/").path);
        assert_eq!("hello/world", RelativePath::new("/hello/world").path);
        assert_eq!("hello/world", RelativePath::new("hello/world").path);
    }
}
