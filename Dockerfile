FROM rust:1.85-bookworm AS builder
WORKDIR /app
COPY . .
RUN cargo build --release -p mnemo-cli

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/mnemo-cli /usr/local/bin/mnemo
ENV MNEMO_DB_PATH=/data/mnemo.db
VOLUME /data
ENTRYPOINT ["mnemo"]
