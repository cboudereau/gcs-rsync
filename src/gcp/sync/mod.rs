mod fs;
mod gcs;

use std::ops::Not;
use std::path::{Path, PathBuf};

use bytes::Bytes;
use futures::future::Either;
use futures::{Future, Stream, StreamExt, TryStreamExt};

use fs::FsClient;
use gcs::GcsClient;
use globset::{Glob, GlobSet, GlobSetBuilder};

use crate::oauth2::token::TokenGenerator;

pub struct ReaderWriter {
    inner: ReaderWriterInternal,
}

pub type Source = ReaderWriter;

impl ReaderWriter {
    fn new(inner: ReaderWriterInternal) -> Self {
        Self { inner }
    }

    pub async fn gcs(
        token_generator: Box<dyn TokenGenerator>,
        bucket: &str,
        prefix: &str,
    ) -> RSyncResult<Self> {
        let client = GcsClient::new(token_generator, bucket, prefix).await?;
        Ok(Self::new(ReaderWriterInternal::Gcs(Box::new(client))))
    }

    pub fn public_gcs(bucket: &str, prefix: &str) -> Self {
        let client = GcsClient::no_auth(bucket, prefix);
        Self::new(ReaderWriterInternal::Gcs(Box::new(client)))
    }

    pub fn fs(base_path: &Path) -> Self {
        let client = FsClient::new(base_path);
        Self::new(ReaderWriterInternal::Fs(Box::new(client)))
    }
}

//TODO: replace this with trait when async trait will be more stable with method returning Trait
enum ReaderWriterInternal {
    Gcs(Box<GcsClient>),
    Fs(Box<FsClient>),
}

type Size = u64;

impl ReaderWriterInternal {
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

    async fn write<S>(
        &self,
        mtime: Option<chrono::DateTime<chrono::Utc>>,
        set_fs_mtime: bool,
        path: &RelativePath,
        stream: S,
    ) -> RSyncResult<()>
    where
        S: futures::TryStream<Ok = bytes::Bytes, Error = RSyncError> + Send + Sync + 'static,
    {
        async {
            match self {
                ReaderWriterInternal::Gcs(client) => match mtime {
                    Some(mtime) => client.write_mtime(mtime, path, stream).await,
                    None => client.write(path, stream).await,
                },
                ReaderWriterInternal::Fs(client) => match (mtime, set_fs_mtime) {
                    (Some(mtime), true) => client.write_mtime(mtime, path, stream).await,
                    _ => client.write(path, stream).await,
                },
            }
        }
        .await
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

    async fn size_and_mt(
        &self,
        path: &RelativePath,
    ) -> RSyncResult<(Option<chrono::DateTime<chrono::Utc>>, Option<Size>)> {
        match self {
            ReaderWriterInternal::Gcs(client) => client.size_and_mt(path).await,
            ReaderWriterInternal::Fs(client) => client.size_and_mt(path).await,
        }
    }
}

pub struct RSync {
    source: ReaderWriterInternal,
    dest: ReaderWriterInternal,
    restore_fs_mtime: bool,
    includes: Option<GlobSet>,
    excludes: Option<GlobSet>,
}

impl RSync {
    pub fn new(source: ReaderWriter, dest: ReaderWriter) -> Self {
        Self {
            source: source.inner,
            dest: dest.inner,
            restore_fs_mtime: false,
            includes: None,
            excludes: None,
        }
    }

    pub fn with_restore_fs_mtime(mut self, restore_fs_mtime: bool) -> Self {
        self.restore_fs_mtime = restore_fs_mtime;
        self
    }

    fn glob_set(globs: &[&str]) -> RSyncResult<Option<GlobSet>> {
        fn glob_error(error: globset::Error) -> RSyncError {
            RSyncError::GlobError(error.to_string())
        }

        if globs.is_empty() {
            Ok(None)
        } else {
            let glob_set = {
                let mut builer = GlobSetBuilder::new();
                for glob in globs {
                    builer.add(Glob::new(glob).map_err(glob_error)?);
                }
                builer.build().map_err(glob_error)?
            };
            Ok(Some(glob_set))
        }
    }

    pub fn with_includes(mut self, includes: &[&str]) -> RSyncResult<Self> {
        self.includes = Self::glob_set(includes)?;
        Ok(self)
    }

    pub fn with_excludes(mut self, excludes: &[&str]) -> RSyncResult<Self> {
        self.excludes = Self::glob_set(excludes)?;
        Ok(self)
    }

    async fn write_entry(
        &self,
        mtime: Option<chrono::DateTime<chrono::Utc>>,
        path: &RelativePath,
    ) -> RSyncResult<()> {
        let source = self.source.read(path).await;
        self.dest
            .write(mtime, self.restore_fs_mtime, path, source)
            .await?;
        Ok(())
    }

    async fn sync_entry_crc32c(&self, path: &RelativePath) -> RSyncResult<RSyncStatus> {
        Ok(match self.dest.get_crc32c(path).await? {
            None => {
                self.write_entry(None, path).await?;
                RSyncStatus::updated("no dest crc32c", path)
            }
            Some(crc32c_dest) => {
                let crc32c_source = self.source.get_crc32c(path).await?;
                if Some(crc32c_dest) == crc32c_source {
                    RSyncStatus::already_synced("same crc32c", path)
                } else {
                    self.write_entry(None, path).await?;
                    RSyncStatus::updated("different crc32c", path)
                }
            }
        })
    }

    async fn sync_entry(&self, path: &RelativePath) -> RSyncResult<RSyncStatus> {
        Ok(match self.dest.size_and_mt(path).await? {
            (Some(dest_dt), Some(dest_size)) => match self.source.size_and_mt(path).await? {
                (Some(source_dt), Some(source_size)) => {
                    let dest_ts = dest_dt.timestamp();
                    let source_ts = source_dt.timestamp();
                    if dest_ts == source_ts && dest_size == source_size {
                        RSyncStatus::already_synced("same mtime and size", path)
                    } else {
                        self.write_entry(Some(source_dt), path).await?;
                        RSyncStatus::updated("different size or mtime", path)
                    }
                }
                _ => self.sync_entry_crc32c(path).await?,
            },
            (None, None) => {
                let (mtime, _) = self.source.size_and_mt(path).await?;
                self.write_entry(mtime, path).await?;
                RSyncStatus::Created(path.to_owned())
            }
            _ => self.sync_entry_crc32c(path).await?,
        })
    }

    fn filter(&self, relative_path: &RelativePath) -> bool {
        let i = self
            .includes
            .as_ref()
            .map(|includes| {
                includes
                    .matches(relative_path.path.as_str())
                    .is_empty()
                    .not()
            })
            .unwrap_or(true);
        let x = self
            .excludes
            .as_ref()
            .map(|excludes| excludes.matches(relative_path.path.as_str()).is_empty())
            .unwrap_or(true);
        i && x
    }

    /// Sync synchronize source to destination by comparing crc32c if destination already exists
    ///
    /// Example
    /// ```rust
    /// use std::{path::PathBuf, str::FromStr};
    ///
    /// use futures::{StreamExt, TryStreamExt};
    /// use gcs_rsync::{
    ///     storage::credentials::authorizeduser,
    ///     sync::{RSync, RSyncResult, ReaderWriter},
    /// };
    ///
    /// #[tokio::main]
    /// async fn main() -> RSyncResult<()> {
    ///     let token_generator = Box::new(authorizeduser::default().await.unwrap());
    ///
    ///     let home_dir = ".";
    ///     let test_prefix = "bucket_prefix_to_sync";
    ///     let bucket = "bucket_name";
    ///
    ///     let source = ReaderWriter::gcs(token_generator, bucket, test_prefix)
    ///         .await
    ///         .unwrap();
    ///
    ///     let dest_folder = {
    ///         let mut p = PathBuf::from_str(home_dir).unwrap();
    ///         p.push(test_prefix);
    ///         p
    ///     };
    ///     let dest = ReaderWriter::fs(dest_folder.as_path());
    ///
    ///     let rsync = RSync::new(source, dest);
    ///
    ///     rsync
    ///         .sync()
    ///         .await
    ///         .try_buffer_unordered(12)
    ///         .for_each(|x| {
    ///             println!("{:?}", x);
    ///             futures::future::ready(())
    ///         })
    ///         .await;
    ///
    ///     Ok(())
    /// }
    /// ```
    pub async fn sync(
        &self,
    ) -> impl Stream<Item = RSyncResult<impl Future<Output = RSyncResult<RSyncStatus>> + '_>> + '_
    {
        self.source
            .list()
            .await
            .try_filter(|x| futures::future::ready(self.filter(x)))
            .map_ok(move |path| async move { self.sync_entry(&path).await })
    }

    async fn delete_extras(
        &self,
    ) -> impl Stream<Item = RSyncResult<impl Future<Output = RSyncResult<RMirrorStatus>> + '_>> + '_
    {
        self.dest.list().await.map(move |result| {
            result.map(|path| async move {
                if self.source.exists(&path).await?.not() || self.filter(&path).not() {
                    self.dest.delete(&path).await?;
                    Ok(RMirrorStatus::Deleted(path))
                } else {
                    Ok(RMirrorStatus::NotDeleted(path))
                }
            })
        })
    }

    /// Mirror synchronize source to destination by deleting extras (destination)
    ///
    /// Example
    /// ```rust
    /// use std::{path::PathBuf, str::FromStr};
    ///
    /// use futures::{StreamExt, TryStreamExt};
    /// use gcs_rsync::{
    ///     storage::credentials::authorizeduser,
    ///     sync::{RSync, RSyncResult, ReaderWriter},
    /// };
    ///
    /// #[tokio::main]
    /// async fn main() -> RSyncResult<()> {
    ///     let token_generator = Box::new(authorizeduser::default().await.unwrap());
    ///
    ///     let home_dir = ".";
    ///     let test_prefix = "bucket_prefix_to_sync";
    ///     let bucket = "bucket_name";
    ///
    ///     let source = ReaderWriter::gcs(token_generator, bucket, test_prefix)
    ///         .await
    ///         .unwrap();
    ///
    ///     let dest_folder = {
    ///         let mut p = PathBuf::from_str(home_dir).unwrap();
    ///         p.push(test_prefix);
    ///         p
    ///     };
    ///     let dest = ReaderWriter::fs(dest_folder.as_path());
    ///
    ///     let rsync = RSync::new(source, dest);
    ///
    ///     rsync
    ///         .mirror()
    ///         .await
    ///         .try_buffer_unordered(12)
    ///         .for_each(|x| {
    ///             println!("{:?}", x);
    ///             futures::future::ready(())
    ///         })
    ///         .await;
    ///
    ///     Ok(())
    /// }
    /// ```
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

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RelativePath {
    path: String,
}

impl std::fmt::Debug for RelativePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "{}", self.path)
    }
}

impl RelativePath {
    /// Invariant: a name should not start with a slash and folder separator is a slash
    pub fn new(path: &str) -> RSyncResult<Self> {
        let path = path
            .strip_prefix('/')
            .unwrap_or(path)
            .replace('\\', "/")
            .to_owned();
        if path.is_empty() {
            Err(RSyncError::EmptyRelativePathError)
        } else {
            Ok(Self { path })
        }
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
    FsIoError {
        path: PathBuf,
        message: String,
        error: std::io::Error,
    },
    EmptyRelativePathError,
    GlobError(String),
}

impl RSyncError {
    fn fs_io_error<T, U>(message: U, path: T, error: std::io::Error) -> RSyncError
    where
        T: AsRef<Path>,
        U: AsRef<str>,
    {
        RSyncError::FsIoError {
            path: path.as_ref().to_path_buf(),
            message: message.as_ref().to_owned(),
            error,
        }
    }
}

impl std::fmt::Display for RSyncError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::error::Error for RSyncError {}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RSyncStatus {
    Created(RelativePath),
    Updated { reason: String, path: RelativePath },
    AlreadySynced { reason: String, path: RelativePath },
}

impl RSyncStatus {
    fn updated(reason: &str, path: &RelativePath) -> Self {
        let reason = reason.to_owned();
        let path = path.to_owned();
        Self::Updated { reason, path }
    }

    fn already_synced(reason: &str, path: &RelativePath) -> Self {
        let reason = reason.to_owned();
        let path = path.to_owned();
        Self::AlreadySynced { reason, path }
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RMirrorStatus {
    Synced(RSyncStatus),
    Deleted(RelativePath),
    NotDeleted(RelativePath),
}

pub type RSyncResult<T> = Result<T, RSyncError>;

#[cfg(test)]
mod tests {
    use crate::{gcp::sync::RelativePath, sync::RSyncError};

    #[test]
    fn test_relative_path() {
        fn is_empty_err(x: RSyncError) -> bool {
            matches!(x, RSyncError::EmptyRelativePathError)
        }
        assert!(
            is_empty_err(RelativePath::new("").unwrap_err()),
            "empty path is not allowed"
        );
        assert!(is_empty_err(RelativePath::new("/").unwrap_err()));
        assert_eq!(
            "hello/world",
            RelativePath::new("/hello/world").unwrap().path
        );
        assert_eq!(
            "hello/world",
            RelativePath::new("hello/world").unwrap().path
        );
    }
}
