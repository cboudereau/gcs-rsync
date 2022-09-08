//! Fast and native Google Cloud Storage rsync
//!
//! - __fast__: gcs-rsync performance are better than gsutil rsync: see [benchmark](https://github.com/cboudereau/gcs-rsync#benchmark)
//! - __native__: gcs-rsync can be used without gcloud components
//! - __gcs auth__: __authorized user__ (dev account) and __service account__
//! - __features__: source and destination can be either fs (_file system_) or gcs (_Google Cloud Storage_) meaning that any combination is allowed
//!
//! __Be careful__ with the __mirror__ feature as it will __delete all extras__ meaning that entry not found in the source is deleted from the destination
//!
//! # Quick Start
//! ```rust
//! use std::{path::PathBuf, str::FromStr};
//!
//! use futures::{StreamExt, TryStreamExt};
//! use gcs_rsync::{
//!     storage::credentials::authorizeduser,
//!     sync::{RSync, RSyncResult, ReaderWriter},
//! };
//!
//! #[tokio::main]
//! async fn main() -> RSyncResult<()> {
//!     let token_generator = Box::new(authorizeduser::default().await.unwrap());
//!
//!     let home_dir = ".";
//!     let test_prefix = "bucket_prefix_to_sync";
//!     let bucket = "bucket_name";
//!
//!     let source = ReaderWriter::gcs(token_generator, bucket, test_prefix)
//!         .await
//!         .unwrap();
//!
//!     let dest_folder = {
//!         let mut p = PathBuf::from_str(home_dir).unwrap();
//!         p.push(test_prefix);
//!         p
//!     };
//!     let dest = ReaderWriter::fs(dest_folder.as_path());
//!
//!     let rsync = RSync::new(source, dest);
//!
//!     rsync
//!         .sync()
//!         .await
//!         .try_buffer_unordered(12)
//!         .for_each(|x| {
//!             println!("{:?}", x);
//!             futures::future::ready(())
//!         })
//!         .await;
//!
//!     Ok(())
//! }
//! ```
//! # Sync and Mirror examples
//! - [`gcp::sync::RSync`] examples
//! - [other examples](https://github.com/cboudereau/gcs-rsync/tree/main/examples)
mod gcp;

pub use gcp::oauth2;
pub use gcp::sync;
pub use gcp::{storage, Client};

const DEFAULT_BUF_SIZE: usize = 64 * 1024;
