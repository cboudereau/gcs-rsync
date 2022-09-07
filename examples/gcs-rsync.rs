use std::{path::Path, str::FromStr};

use futures::{StreamExt, TryStreamExt};
use gcs_rsync::{
    oauth2::token::TokenGenerator,
    storage::{
        credentials::{authorizeduser, metadata},
        Object,
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

    /// Activate mirror mode (sync by default)
    #[structopt(short, long)]
    mirror: bool,

    /// Restore mtime on filesystem (disabled by default)
    #[structopt(short, long)]
    restore_fs_mtime: bool,

    /// Source path: can be either gs (gs://bucket/path/to/object) or fs source
    #[structopt()]
    source: String,

    /// Destination path: can be either gs (gs://bucket/path/to/object) or fs source
    #[structopt()]
    dest: String,
}

async fn get_source(
    path: &str,
    is_dest: bool,
    use_metadata_token_api: bool,
) -> RSyncResult<Source> {
    match Object::from_str(path).ok() {
        Some(o) => {
            let token_generator: Box<dyn TokenGenerator> = if use_metadata_token_api {
                Box::new(metadata::default().map_err(RSyncError::StorageError)?)
            } else {
                Box::new(
                    authorizeduser::default()
                        .await
                        .map_err(RSyncError::StorageError)?,
                )
            };
            Source::gcs(token_generator, o.bucket.as_str(), o.name.as_str()).await
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

    let rsync = RSync::new(source, dest).with_restore_fs_mtime(opt.restore_fs_mtime);

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
