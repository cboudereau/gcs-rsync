//! Fast and native Google Cloud Storage rsync
//!
//! - fast: gcs-rsync performance are better than gsutil rsync: see [benchmark](https://github.com/cboudereau/gcs-rsync#benchmark)
//! - native: gcs-rsync can be used without gcloud components
//! - gcs auth supported: authorized user (dev account) and service account
//! - features: source and destination can be either fs (file system) or gcs (Google Cloud Storage) meaning that any combination is allowed
//!
//! Be careful with the mirror feature as it will delete all extras meaning that entry not found in the source is deleted from the destination
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
//!     let token_generator = authorizeduser::default().await.unwrap();
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
mod gcp;

pub use gcp::oauth2;
pub use gcp::sync;
pub use gcp::{storage, Client};
