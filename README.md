# gcs-rsync

Lightweight and efficient faster than [Google Cloud Storage rsync](https://cloud.google.com/storage/docs/gsutil/commands/rsync) for Rust.

gcs-sync is faster than `gsutil rsync` when files change a lot while performance is similar to `gsutil` when there is no changes.

- [x] OAuth2 service account (default, from and from_file)
- [x] OAuth2 dev (default, from and from_file)
- [x] OAuth2 Integration tests + examples
- [x] Useful diagnostic on error (raw json response)
- [x] List objects with better performance than gsutil by supporting [GCS Partial Response](https://cloud.google.com/storage/docs/json_api#partial-response)
- [x] Upload/Download/Get/Delete objects + Integrations tests and examples
- [x] Sync local folder (one way sync without delete remote files with crc32c support)
- [x] Mirror local folder (sync + delete remotes files)
- [x] Benchmarks
- [x] Sync/Mirror integration tests
- [x] Doc crate
- [x] CI/CD
- [ ] Publish crate

### Benchmark

Important note about gsutil: The `gsutils ls` command does not list all object items by default but instead list all prefixes while adding the `-r` flag slowdown `gsutil` performance. The `ls` performance command is very different to the `rsync` implementation.

#### new files only (first time sync)

- gcs-sync: 2.2s/7MB
- gsutil: 9.93s/47MB

**winner**: gcs-sync

##### gcs-sync sync bench

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

##### gsutil sync bench

```bash
rm -rf ~/Documents/gsutil_test4 && mkdir ~/Documents/gsutil_test4 && /usr/bin/time -lp --  gsutil -m rsync -r gs://dev-bucket/sync_test4/ ~/Documents/gsutil_test4/
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

#### no change (second time sync)

- gcs-sync: 1.79s/8MB
- gsutil: 2.18s/47MB

**winner**: no clear winner, but at least gcs-sync perf is similar to `gsutil rync` when there is no modification (which is quite rare).

##### gcs-sync sync bench

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

##### gsutil sync bench

```bash
/usr/bin/time -lp --  gsutil -m rsync -r gs://test-bucket/sync_test4/ ~/Documents/gsutil_test4/
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

#### gsutil rsync config

```bash
gsutil -m rsync -r -d ./your-dir gs://your-bucket
```

```bash
/usr/bin/time -lp --  gsutil -m rsync -r gs://dev-bucket/sync_test4/ ~/Documents/gsutil_test4/
```

- [commands](https://cloud.google.com/storage/docs/gsutil/commands/rsync)
- [crc32c for gsutil](https://cloud.google.com/storage/docs/gsutil/addlhelp/CRC32CandInstallingcrcmod)

# About authentication

All default functions related to authentication use GOOGLE_APPLICATION_CREDENTIALS env var as default conf like official Google libraries do on other languages (golang, dotnet)

Other functions (from and from_file) provide the custom integration mode.

For more info about OAuth2, see the related README in the oauth2 mod.

# How to run tests

## Unit tests

```bash
cargo test --lib
```

## Integration tests + Unit tests

```bash
TEST_SERVICE_ACCOUNT=<PathToAServiceAccount> TEST_BUCKET=<BUCKET> TEST_PREFIX=<PREFIX> cargo test --no-fail-fast
```

## Example

### Upload object

```bash
cargo run --release --example upload_object "<YourBucket>" "<YourPrefix>" "<YourFilePath>"
```

### Download object

```bash
cargo run --release --example download_object "<YourBucket>" "<YourObjectName>" "<YourAbsoluteExistingDirectory>"
```

### Delete object

```bash
cargo run --release --example delete_object "<YourBucket>" "<YourPrefix>/<YourFileName>"
```

### List objects

```bash
cargo run --release --example list_objects "<YourBucket>" "<YourPrefix>"
```

### List objects with default service account

```bash
GOOGLE_APPLICATION_CREDENTIALS=<PathToJson> cargo r --release --example list_objects_service_account "<YourBucket>" "<YourPrefix>"
```

#### List objects

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
