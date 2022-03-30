ARG IMAGE=rust:1.59.0-slim-buster

FROM $IMAGE as planner
WORKDIR app
RUN cargo install cargo-chef
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM $IMAGE as cacher
WORKDIR app
RUN cargo install cargo-chef
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

FROM $IMAGE as builder
WORKDIR app
COPY . .
COPY --from=cacher /app/target target
RUN apt-get update && \
    apt-get install -y musl-tools ca-certificates && \
    rustup target add x86_64-unknown-linux-musl
RUN RUSTFLAGS="-C target-feature=+crt-static -C link-self-contained=yes" cargo install --target x86_64-unknown-linux-musl --path .

FROM scratch AS runtime
ENV RUST_LOG=info
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/s3proxy /usr/local/bin/app
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
ENTRYPOINT ["/usr/local/bin/app"]
