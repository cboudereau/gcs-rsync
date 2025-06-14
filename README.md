# gcs-rsync

[![build](https://github.com/cboudereau/gcs-rsync/workflows/build/badge.svg)](https://github.com/cboudereau/gcs-rsync/actions/workflows/build.yml?query=event%3Apush)
[![codecov](https://codecov.io/gh/cboudereau/gcs-rsync/branch/main/graph/badge.svg?token=AA168CFNFQ)](https://codecov.io/gh/cboudereau/gcs-rsync)
[![License:MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![docs.rs](https://docs.rs/gcs-rsync/badge.svg)](https://docs.rs/gcs-rsync)
[![crates.io](https://img.shields.io/crates/v/gcs-rsync.svg)](https://crates.io/crates/gcs-rsync)
[![crates.io (recent)](https://img.shields.io/crates/dr/gcs-rsync)](https://crates.io/crates/gcs-rsync)
[![docker](https://img.shields.io/docker/pulls/superbeeeeeee/gcs-rsync)](https://hub.docker.com/r/superbeeeeeee/gcs-rsync)

Lightweight and efficient Rust gcs rsync for Google Cloud Storage.

gcs-rsync is faster than [gsutil rsync](https://cloud.google.com/storage/docs/gsutil/commands/rsync) according to the following benchmarks.

no hard limit to 32K objects or specific conf to compute state.

This crate can be used as a library or CLI. The API managing objects (download, upload, delete, ...) can be used independently.

## How to install as crate

Cargo.toml

```bash
[dependencies]
gcs-rsync = "0.4"
```

## How to install as cli tool

```bash
cargo install --example gcs-rsync gcs-rsync

~/.cargo/bin/gcs-rsync

```

## How to run with docker

### Mirror local folder to gcs

```bash
docker run --rm -it -v ${GOOGLE_APPLICATION_CREDENTIALS}:/creds.json:ro -v <YourFolderToUpload>:/source:ro superbeeeeeee/gcs-rsync -r -m /source gs://<YourBucket>/<YourFolderToUpload>/
```

### Mirror gcs to folder

```bash
docker run --rm -it -v ${GOOGLE_APPLICATION_CREDENTIALS}:/creds.json:ro -v <YourFolderToDownloadTo>:/dest superbeeeeeee/gcs-rsync -r -m gs://<YourBucket>/<YourFolderToUpload>/ /dest
```

### Mirror partial gcs with prefix to folder

```bash
docker run --rm -it -v ${GOOGLE_APPLICATION_CREDENTIALS}:/creds.json:ro -v <YourFolderToDownloadTo>:/dest superbeeeeeee/gcs-rsync -r -m gs://<YourBucket>/<YourFolderToUpload>/<YourPrefix> /dest
```

### Include or Exclude files using glob pattern

#### CLI gcs-rsync

```-i``` (include glob pattern) and ```-x``` (exclude glob pattern) multiple times.

An example where any json or toml are included recursively except any test.json or test.toml recursively
```bash
docker run --rm -it -v ${GOOGLE_APPLICATION_CREDENTIALS}:/creds.json:ro -v <YourFolderToDownloadTo>:/dest superbeeeeeee/gcs-rsync -r -m -i **/*.json -i **/*.toml -x **/test.json -x **/test.toml
 gs://<YourBucket>/YourFolderToUpload>/ /dest
```

#### Library
```with_includes``` and ```with_excludes``` client builders are used to fill includes and excludes glob patterns.

## Testing
By configuring the env var `STORAGE_EMULATOR_HOST` (defaulted to `https://storage.googleapis.com`), a gcs emulator can be configured to work properly with this library. 

## Benchmark

Important note about gsutil: The `gsutil ls` command does not list all object items by default but instead list all prefixes while adding the `-r` flag slowdown `gsutil` performance. The `ls` performance command is very different to the `rsync` implementation.

### new files only (first time sync)

- gcs-rsync: 2.2s/7MB
- gsutil: 9.93s/47MB

**winner**: gcs-rsync

#### gcs-rsync sync bench

```bash
rm -rf ~/Documents/test4 && cargo build --release --examples && /usr/bin/time -lp -- ./target/release/examples/bucket_to_folder_sync
```

```
real         2.20
user         0.13
sys          0.21
             7606272  maximum resident set size
                   0  average shared memory size
                   0  average unshared data size
                   0  average unshared stack size
                1915  page reclaims
                   0  page faults
                   0  swaps
                   0  block input operations
                   0  block output operations
                 394  messages sent
                1255  messages received
                   0  signals received
                  54  voluntary context switches
                5814  involuntary context switches
           636241324  instructions retired
           989595729  cycles elapsed
             3895296  peak memory footprint
```

#### gsutil sync bench

```bash
rm -rf ~/Documents/gsutil_test4 && mkdir ~/Documents/gsutil_test4 && /usr/bin/time -lp --  gsutil -m -q rsync -r gs://dev-bucket/sync_test4/ ~/Documents/gsutil_test4/
```

```
Operation completed over 215 objects/50.3 KiB.
real         9.93
user         8.12
sys          2.35
            47108096  maximum resident set size
                   0  average shared memory size
                   0  average unshared data size
                   0  average unshared stack size
              196391  page reclaims
                   1  page faults
                   0  swaps
                   0  block input operations
                   0  block output operations
               36089  messages sent
               87309  messages received
                   5  signals received
               38401  voluntary context switches
               51924  involuntary context switches
            12986389  instructions retired
            12032672  cycles elapsed
              593920  peak memory footprint
```

### no change (second time sync)

- gcs-rsync: 0.78s/8MB
- gsutil: 2.18s/47MB

**winner**: gcs-rsync (due to size and mtime check before crc32c like gsutil does)

#### gcs-rsync sync bench

```bash
cargo build --release --examples && /usr/bin/time -lp -- ./target/release/examples/bucket_to_folder_sync
```

```
real         1.79
user         0.13
sys          0.12
             7864320  maximum resident set size
                   0  average shared memory size
                   0  average unshared data size
                   0  average unshared stack size
                1980  page reclaims
                   0  page faults
                   0  swaps
                   0  block input operations
                   0  block output operations
                 397  messages sent
                1247  messages received
                   0  signals received
                  42  voluntary context switches
                4948  involuntary context switches
           435013936  instructions retired
           704782682  cycles elapsed
             4141056  peak memory footprint
```

#### gsutil sync bench

```bash
/usr/bin/time -lp --  gsutil -m -q rsync -r gs://test-bucket/sync_test4/ ~/Documents/gsutil_test4/
```

```
real         2.18
user         1.37
sys          0.66
            46899200  maximum resident set size
                   0  average shared memory size
                   0  average unshared data size
                   0  average unshared stack size
              100108  page reclaims
                1732  page faults
                   0  swaps
                   0  block input operations
                   0  block output operations
                6311  messages sent
               12752  messages received
                   4  signals received
                6145  voluntary context switches
               14219  involuntary context switches
            13133297  instructions retired
            13313536  cycles elapsed
              602112  peak memory footprint
```

### gsutil rsync config

```bash
gsutil -m -q rsync -r -d ./your-dir gs://your-bucket
```

```bash
/usr/bin/time -lp --  gsutil -m -q rsync -r gs://dev-bucket/sync_test4/ ~/Documents/gsutil_test4/
```

- [commands](https://cloud.google.com/storage/docs/gsutil/commands/rsync)
- [crc32c for gsutil](https://cloud.google.com/storage/docs/gsutil/addlhelp/CRC32CandInstallingcrcmod)

## About authentication

All default functions related to authentication use GOOGLE_APPLICATION_CREDENTIALS env var as default conf like official Google libraries do on other languages (golang, dotnet)

Other functions (from and from_file) provide the custom integration mode.

For more info about OAuth2, see the related README in the oauth2 mod.

## How to run tests

## Unit tests

```bash
cargo test --lib
```

## Integration tests + Unit tests

```bash
TEST_SERVICE_ACCOUNT=<PathToAServiceAccount> TEST_BUCKET=<BUCKET> TEST_PREFIX=<PREFIX> cargo test --no-fail-fast
```

## Examples

### Upload object

#### Library
```rust
use std::path::Path;

use gcs_rsync::storage::{credentials, Object, ObjectClient, StorageResult};
use tokio_util::codec::{BytesCodec, FramedRead};

#[tokio::main]
async fn main() -> StorageResult<()> {
    let args = std::env::args().collect::<Vec<_>>();
    let bucket = args[1].as_str();
    let prefix = args[2].to_owned();
    let file_path = args[3].to_owned();

    let auc = Box::new(credentials::authorizeduser::default().await?);
    let object_client = ObjectClient::new(auc).await?;

    let file_path = Path::new(&file_path);
    let name = file_path.file_name().unwrap().to_string_lossy();

    let file = tokio::fs::File::open(file_path).await.unwrap();
    let stream = FramedRead::new(file, BytesCodec::new());

    let name = format!("{}/{}", prefix, name);
    let object = Object::new(bucket, name.as_str())?;
    object_client.upload(&object, stream).await.unwrap();
    println!("object {} uploaded", &object);
    Ok(())
}
```

#### CLI

```bash
cargo run --release --example upload_object "<YourBucket>" "<YourPrefix>" "<YourFilePath>"
```

### Download object

#### Library
```rust
use std::path::Path;

use futures::TryStreamExt;
use gcs_rsync::storage::{credentials, Object, ObjectClient, StorageResult};
use tokio::{
    fs::File,
    io::{AsyncWriteExt, BufWriter},
};

#[tokio::main]
async fn main() -> StorageResult<()> {
    let args = std::env::args().collect::<Vec<_>>();
    let bucket = args[1].as_str();
    let name = args[2].as_str();
    let output_path = args[3].to_owned();

    let auc = Box::new(credentials::authorizeduser::default().await?);
    let object_client = ObjectClient::new(auc).await?;

    let file_name = Path::new(&name).file_name().unwrap().to_string_lossy();
    let file_path = format!("{}/{}", output_path, file_name);

    let object = Object::new(bucket, name)?;
    let mut stream = object_client.download(&object).await.unwrap();

    let file = File::create(&file_path).await.unwrap();
    let mut buf_writer = BufWriter::new(file);

    while let Some(data) = stream.try_next().await.unwrap() {
        buf_writer.write_all(&data).await.unwrap();
    }

    buf_writer.flush().await.unwrap();
    println!("object {} downloaded to {:?}", &object, file_path);
    Ok(())
}
```

#### CLI
```bash
cargo run --release --example download_object "<YourBucket>" "<YourObjectName>" "<YourAbsoluteExistingDirectory>"
```

### Download public object

#### Library
```rust
use std::path::Path;

use futures::TryStreamExt;
use gcs_rsync::storage::{Object, ObjectClient, StorageResult};
use tokio::{
    fs::File,
    io::{AsyncWriteExt, BufWriter},
};

#[tokio::main]
async fn main() -> StorageResult<()> {
    let bucket = "gcs-rsync-dev-public";
    let name = "hello.txt";

    let object_client = ObjectClient::no_auth();

    let file_name = Path::new(&name).file_name().unwrap().to_string_lossy();
    let file_path = file_name.to_string();

    let object = Object::new(bucket, "hello.txt")?;
    let mut stream = object_client.download(&object).await.unwrap();

    let file = File::create(&file_path).await.unwrap();
    let mut buf_writer = BufWriter::new(file);

    while let Some(data) = stream.try_next().await.unwrap() {
        buf_writer.write_all(&data).await.unwrap();
    }

    buf_writer.flush().await.unwrap();
    println!("object {} downloaded to {:?}", &object, file_path);
    Ok(())
}
```

#### CLI
```bash
cargo run --release --example download_public_object "<YourBucket>" "<YourObjectName>" "<YourAbsoluteExistingDirectory>"
```

### Delete object

#### Library
```rust
use gcs_rsync::storage::{credentials, Object, ObjectClient, StorageResult};

#[tokio::main]
async fn main() -> StorageResult<()> {
    let args = std::env::args().collect::<Vec<_>>();
    let bucket = args[1].as_str();
    let name = args[2].as_str();
    let object = Object::new(bucket, name)?;

    let auc = Box::new(credentials::authorizeduser::default().await?);
    let object_client = ObjectClient::new(auc).await?;

    object_client.delete(&object).await?;
    println!("object {} uploaded", &object);
    Ok(())
}
```

#### CLI

```bash
cargo run --release --example delete_object "<YourBucket>" "<YourPrefix>/<YourFileName>"
```

### List objects

#### Library
```rust
use futures::TryStreamExt;
use gcs_rsync::storage::{credentials, ObjectClient, ObjectsListRequest, StorageResult};

#[tokio::main]
async fn main() -> StorageResult<()> {
    let args = std::env::args().collect::<Vec<_>>();
    let bucket = args[1].as_str();
    let prefix = args[2].to_owned();

    let auc = Box::new(credentials::authorizeduser::default().await?);
    let object_client = ObjectClient::new(auc).await?;

    let objects_list_request = ObjectsListRequest {
        prefix: Some(prefix),
        fields: Some("items(name),nextPageToken".to_owned()),
        ..Default::default()
    };

    object_client
        .list(bucket, &objects_list_request)
        .await
        .try_for_each(|x| {
            println!("{}", x.name.unwrap());
            futures::future::ok(())
        })
        .await?;

    Ok(())
}
```

#### CLI

```bash
cargo run --release --example list_objects "<YourBucket>" "<YourPrefix>"
```

### List objects with default service account

#### Library
```rust
use futures::TryStreamExt;
use gcs_rsync::storage::{credentials, ObjectClient, ObjectsListRequest, StorageResult};

#[tokio::main]
async fn main() -> StorageResult<()> {
    let args = std::env::args().collect::<Vec<_>>();
    let bucket = args[1].as_str();
    let prefix = args[2].to_owned();

    let auc = Box::new(
        credentials::serviceaccount::default(
            "https://www.googleapis.com/auth/devstorage.full_control",
        )
        .await?,
    );
    let object_client = ObjectClient::new(auc).await?;

    let objects_list_request = ObjectsListRequest {
        prefix: Some(prefix),
        fields: Some("items(name),nextPageToken".to_owned()),
        ..Default::default()
    };

    object_client
        .list(bucket, &objects_list_request)
        .await
        .try_for_each(|x| {
            println!("{}", x.name.unwrap());
            futures::future::ok(())
        })
        .await?;

    Ok(())
}
```

#### CLI
```bash
GOOGLE_APPLICATION_CREDENTIALS=<PathToJson> cargo r --release --example list_objects_service_account "<YourBucket>" "<YourPrefix>"
```

#### List lots of (>32K) objects

list a bucket having more than 60K objects

```bash
time cargo run --release --example list_objects "<YourBucket>" "<YourPrefixHavingMoreThan60K>" | wc -l
```

### Profiling

[Humans are terrible at guessing-about-performance](https://github.com/flamegraph-rs/flamegraph#humans-are-terrible-at-guessing-about-performance)

```bash
export CARGO_PROFILE_RELEASE_DEBUG=true
sudo -- cargo flamegraph --example list_objects "<YourBucket>" "<YourPrefixHavingMoreThan60K>"
```

```bash
cargo build --release --examples && /usr/bin/time -lp -- ./target/release/examples/list_objects "<YourBucket>" "<YourPrefixHavingMoreThan60K>"
```

### Native bin build (static shared lib)

```bash
docker rust rust:alpine3.14
apk add --no-cache musl-dev pkgconfig openssl-dev

LDFLAGS="-static -L/usr/local/musl/lib" LD_LIBRARY_PATH=/usr/local/musl/lib:$LD_LIBRARY_PATH CFLAGS="-I/usr/local/musl/include" PKG_CONFIG_PATH=/usr/local/musl/lib/pkgconfig cargo build --release --target=x86_64-unknown-linux-musl --example bucket_to_folder_sync
```
