use std::{path::Path, str::FromStr};

use futures::{StreamExt, TryStreamExt};
use gcs_rsync::{
    oauth2::token::TokenGenerator,
    storage::{
        credentials::{authorizeduser, metadata},
        Error, StorageResult,
    },
    sync::{RSync, RSyncError, RSyncResult, Source},
};

use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "gcs-rsync",
    about = "synchronize fs or gcs source to destination"
)]
struct Opt {
    /// Use Google metadata api for authentication
    #[structopt(short, long)]
    use_metadata_token_api: bool,

    /// Activate mirror mode (sync by default). /!\ This mode will deletes all extra entries.
    #[structopt(short, long)]
    mirror: bool,

    /// Restore mtime on filesystem (disabled by default)
    #[structopt(short, long)]
    restore_fs_mtime: bool,

    /// Include glob pattern, can be repeated
    #[structopt(short = "i", long = "include")]
    includes: Vec<String>,

    /// Exclude glob pattern, can be repeated
    #[structopt(short = "x", long = "exclude")]
    excludes: Vec<String>,

    /// Source path: can be either gs (gs://bucket/path/to/object) or fs source
    #[structopt()]
    source: String,

    /// Destination path: can be either gs (gs://bucket/path/to/object) or fs source
    #[structopt()]
    dest: String,
}

#[derive(Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BucketPrefix {
    pub bucket: String,
    pub prefix: String,
}

impl FromStr for BucketPrefix {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.strip_prefix("gs://")
            .and_then(|part| part.split_once('/'))
            .ok_or(Error::GcsInvalidUrl {
                url: s.to_owned(),
                message: "gs url should be gs://bucket/object/path/name".to_owned(),
            })
            .and_then(|(bucket, prefix)| BucketPrefix::new(bucket, prefix))
    }
}

impl BucketPrefix {
    pub fn new(bucket: &str, prefix: &str) -> StorageResult<Self> {
        if bucket.is_empty() {
            return Err(Error::GcsInvalidObjectName);
        }

        Ok(Self {
            bucket: bucket.to_owned(),
            prefix: prefix.to_owned(),
        })
    }
}

async fn get_source(
    path: &str,
    is_dest: bool,
    use_metadata_token_api: bool,
) -> RSyncResult<Source> {
    match BucketPrefix::from_str(path).ok() {
        Some(o) => {
            let token_generator: Option<Box<dyn TokenGenerator>> = if use_metadata_token_api {
                Some(Box::new(
                    metadata::default().map_err(RSyncError::StorageError)?,
                ))
            } else {
                let token_generator = authorizeduser::default().await;
                match token_generator {
                    Err(_) => {
                        println!("no default auth found, running gcs-rsync without auth");
                        None
                    }
                    Ok(o) => Some(Box::new(o)),
                }
            };
            let bucket = o.bucket.as_str();
            let prefix = o.prefix.as_str();
            match token_generator {
                None => Ok(Source::gcs_no_auth(bucket, prefix)),
                Some(token_generator) => Source::gcs(token_generator, bucket, prefix).await,
            }
        }
        None => {
            let path = Path::new(path);
            if path.exists() || is_dest {
                Ok(Source::fs(path))
            } else {
                Err(RSyncError::EmptyRelativePathError)
            }
        }
    }
}

#[tokio::main]
async fn main() -> RSyncResult<()> {
    let num_cpus = num_cpus::get();

    let opt = Opt::from_args();
    let source = get_source(&opt.source, false, opt.use_metadata_token_api).await?;
    let dest = get_source(&opt.dest, true, opt.use_metadata_token_api).await?;

    let rsync = RSync::new(source, dest)
        .with_restore_fs_mtime(opt.restore_fs_mtime)
        .with_includes(
            opt.includes
                .iter()
                .map(String::as_ref)
                .collect::<Vec<_>>()
                .as_slice(),
        )?
        .with_excludes(
            opt.excludes
                .iter()
                .map(String::as_ref)
                .collect::<Vec<_>>()
                .as_slice(),
        )?;

    if opt.mirror {
        println!("mirroring {} > {}", &opt.source, &opt.dest);
        rsync
            .mirror()
            .await
            .try_buffer_unordered(num_cpus)
            .for_each(|x| {
                println!("{:?}", x);
                futures::future::ready(())
            })
            .await;
    } else {
        println!("syncing {} > {}", &opt.source, &opt.dest);
        rsync
            .sync()
            .await
            .try_buffer_unordered(num_cpus)
            .for_each(|x| {
                println!("{:?}", x);
                futures::future::ready(())
            })
            .await;
    };
    Ok(())
}
