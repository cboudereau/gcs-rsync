FROM rust:alpine as builder
RUN apk add --no-cache musl-dev pkgconfig openssl-dev
COPY . /app 
WORKDIR /app

RUN cargo build --release --target=x86_64-unknown-linux-musl --example gcs-rsync

FROM scratch
ENV GOOGLE_APPLICATION_CREDENTIALS=/creds.json
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/examples/gcs-rsync /app/gcs-rsync
WORKDIR /app
ENTRYPOINT [ "/app/gcs-rsync" ]
