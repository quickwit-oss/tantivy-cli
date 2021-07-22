FROM rust:1.53.0 as builder
WORKDIR /usr/src
RUN rustup target add x86_64-unknown-linux-musl

RUN USER=root cargo new tantivy-cli
WORKDIR /usr/src/tantivy-cli
COPY Cargo.toml Cargo.lock ./
RUN cargo build --release

COPY src ./src
RUN cargo install --target x86_64-unknown-linux-musl --path .

FROM scratch
COPY --from=builder /usr/local/cargo/bin/tantivy .
USER 1000
EXPOSE 3000
ENTRYPOINT ["./tantivy"]