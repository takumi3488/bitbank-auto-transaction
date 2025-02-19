FROM rust:1.84.1-bookworm AS builder

WORKDIR /usr/src/app
COPY . .

RUN cargo build --release


FROM gcr.io/distroless/cc-debian12

ENV TZ=Asia/Tokyo
COPY --from=builder /usr/src/app/target/release/bitbank-auto-transaction /usr/local/bin/bitbank-auto-transaction

CMD ["bitbank-auto-transaction"]
