use std::{path::Path, str::FromStr};

use futures::{StreamExt, TryStreamExt};
use gcs_rsync::{
    storage::{credentials::authorizeduser, Object},
    sync::{DefaultSource, RSync, RSyncError, RSyncResult},
};

use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "gcs-rsync",
    about = "synchronize fs or gcs source to destination"
)]
struct Opt {
    /// Activate mirror mode (sync by default)
    #[structopt(short, long)]
    mirror: bool,

    /// Source path: can be either gs (gs://bucket/path/to/object) or fs source
    #[structopt()]
    source: String,

    /// Destination path: can be either gs (gs://bucket/path/to/object) or fs source
    #[structopt()]
    dest: String,
}

async fn get_source(path: &str) -> RSyncResult<DefaultSource> {
    match Object::from_str(path).ok() {
        Some(o) => {
            let token_generator = authorizeduser::default()
                .await
                .map_err(RSyncError::StorageError)?;
            DefaultSource::gcs(token_generator, o.bucket.as_str(), o.name.as_str()).await
        }
        None => {
            let path = Path::new(path);
            if path.exists() {
                Ok(DefaultSource::fs(path))
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

    let source = get_source(&opt.source).await?;
    let dest = get_source(&opt.dest).await?;

    let rsync = RSync::new(source, dest);

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
