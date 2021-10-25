use std::path::{Path, PathBuf};

use bytes::Bytes;
use futures::{Stream, TryStream, TryStreamExt};
use tokio::{
    fs,
    io::{AsyncWriteExt, BufWriter},
};
use tokio_util::codec::{BytesCodec, FramedRead};

use crate::sync::RSyncError;

use super::{Entry, RSyncResult, RelativePath};

struct FsPrefix {
    base_path: PathBuf,
}

impl FsPrefix {
    fn new(base_path: &Path) -> Self {
        let base_path = base_path.to_path_buf();
        Self { base_path }
    }

    fn as_relative_path(&self, name: &Path) -> RSyncResult<RelativePath> {
        let path = name
            .strip_prefix(self.base_path.as_path())
            .unwrap_or(name)
            .to_string_lossy();
        RelativePath::new(&path)
    }

    fn as_file_path(&self, relative_path: &RelativePath) -> PathBuf {
        let mut path = self.base_path.clone();
        path.push(relative_path.path.as_str());
        path
    }
}

pub(super) struct FsClient {
    prefix: FsPrefix,
}

type Size = u64;

impl FsClient {
    pub(super) fn new(base_path: &Path) -> Self {
        let prefix = FsPrefix::new(base_path);
        Self { prefix }
    }

    pub(super) async fn list(&self) -> impl Stream<Item = RSyncResult<RelativePath>> + '_ {
        futures::stream::try_unfold(
            vec![self.prefix.base_path.to_owned()],
            move |mut state| async move {
                match state.pop() {
                    None => Ok(None),
                    Some(path) => {
                        let path = path.as_path();
                        let mut read_dir = tokio::fs::read_dir(path)
                            .await
                            .map_err(|err| RSyncError::fs_io_error("read dir failed", path, err))?;
                        let mut files = Vec::new();
                        while let Some(entry) = read_dir.next_entry().await.map_err(|err| {
                            RSyncError::fs_io_error("next entry failed", path, err)
                        })? {
                            let metadata = entry.metadata().await.map_err(|err| {
                                RSyncError::fs_io_error(
                                    "reading metadata failed",
                                    entry.path(),
                                    err,
                                )
                            })?;
                            if metadata.is_dir() {
                                state.push(entry.path());
                            } else {
                                files.push(self.prefix.as_relative_path(entry.path().as_path()));
                            }
                        }
                        Ok(Some((futures::stream::iter(files), state)))
                    }
                }
            },
        )
        .try_flatten()
    }

    pub(super) async fn read(&self, path: &RelativePath) -> impl Stream<Item = RSyncResult<Bytes>> {
        let path = self.prefix.as_file_path(path);
        let file = fs::File::open(path.as_path()).await.unwrap();
        FramedRead::with_capacity(file, BytesCodec::new(), crate::DEFAULT_BUF_SIZE)
            .map_err(move |err| RSyncError::fs_io_error("read failure", path.as_path(), err))
            .map_ok(|x| x.freeze())
    }

    pub(super) async fn get_crc32c(&self, path: &RelativePath) -> RSyncResult<Option<Entry>> {
        let file_path = self.prefix.as_file_path(path);

        if let Ok(file) = fs::File::open(file_path.as_path()).await {
            let mut frame =
                FramedRead::with_capacity(file, BytesCodec::new(), crate::DEFAULT_BUF_SIZE);

            let mut crc32c: u32 = 0;
            while let Some(data) = frame
                .try_next()
                .await
                .map_err(|e| RSyncError::fs_io_error("crc32c failed", file_path.as_path(), e))?
            {
                crc32c = crc32c::crc32c_append(crc32c, &data);
            }

            Ok(Some(Entry::new(path, crc32c)))
        } else {
            Ok(None)
        }
    }

    pub(super) async fn exists(&self, path: &RelativePath) -> RSyncResult<bool> {
        let path = self.prefix.as_file_path(path);
        Ok(fs::metadata(path.as_path()).await.is_ok())
    }

    pub(super) async fn size_and_mt(
        &self,
        path: &RelativePath,
    ) -> RSyncResult<Option<(chrono::DateTime<chrono::Utc>, Size)>> {
        let path = self.prefix.as_file_path(path);
        match fs::metadata(path.as_path()).await {
            Ok(m) => {
                let mtime = m.modified().map_err(|e| {
                    RSyncError::fs_io_error("file modified time failed", path.as_path(), e)
                })?;
                let size = m.len();
                Ok(Some((mtime.into(), size)))
            }
            _ => Ok(None),
        }
    }

    pub(super) async fn delete(&self, path: &RelativePath) -> RSyncResult<()> {
        let file_path = self.prefix.as_file_path(path);
        fs::remove_file(file_path.as_path())
            .await
            .map_err(|e| RSyncError::fs_io_error("remove file failed", file_path.as_path(), e))
    }

    async fn _write<S>(&self, file_path: &Path, mut stream: S) -> RSyncResult<()>
    where
        S: TryStream<Ok = Bytes, Error = RSyncError> + std::marker::Unpin,
    {
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| RSyncError::fs_io_error("create dir all failed", parent, e))?
        }

        let file = fs::File::create(file_path)
            .await
            .map_err(|e| RSyncError::fs_io_error("create file failed", file_path, e))?;

        let mut buf_writer = BufWriter::with_capacity(crate::DEFAULT_BUF_SIZE, file);

        while let Some(data) = stream.try_next().await? {
            buf_writer.write_all(&data).await.map_err(|e| {
                RSyncError::fs_io_error("buffered write to file failed", file_path, e)
            })?;
        }

        buf_writer
            .flush()
            .await
            .map_err(|e| RSyncError::fs_io_error("buffer flush to file failed", file_path, e))?;

        Ok(())
    }

    fn set_mtime(path: &Path, mtime: chrono::DateTime<chrono::Utc>) -> RSyncResult<()> {
        filetime::set_file_mtime(path, filetime::FileTime::from_system_time(mtime.into()))
            .map_err(|e| RSyncError::fs_io_error("set_mtime failed", path, e))
    }

    pub(super) async fn write<S>(&self, path: &RelativePath, stream: S) -> RSyncResult<()>
    where
        S: TryStream<Ok = Bytes, Error = RSyncError> + std::marker::Unpin,
    {
        let file_path = self.prefix.as_file_path(path);
        self._write(file_path.as_path(), stream).await
    }

    pub(super) async fn write_mtime<S>(
        &self,
        mtime: chrono::DateTime<chrono::Utc>,
        path: &RelativePath,
        stream: S,
    ) -> RSyncResult<()>
    where
        S: TryStream<Ok = Bytes, Error = RSyncError> + std::marker::Unpin,
    {
        let file_path = self.prefix.as_file_path(path);
        let file_path = file_path.as_path();
        self._write(file_path, stream).await?;
        Self::set_mtime(file_path, mtime)?;

        Ok(())
    }
}
