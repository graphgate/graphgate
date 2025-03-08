FROM rust:alpine AS builder

ARG TARGETPLATFORM

RUN apk add --no-cache ca-certificates musl-dev openssl-dev openssl-libs-static \
    && update-ca-certificates

WORKDIR /graphgate
COPY ./ .

RUN if [ "$TARGETPLATFORM" = "linux/amd64" ]; then ARCHITECTURE=x86_64; elif [ "$TARGETPLATFORM" = "linux/arm64" ]; then ARCHITECTURE=aarch64; fi \
    && rustup target add ${ARCHITECTURE}-unknown-linux-musl \
    && cargo build --target ${ARCHITECTURE}-unknown-linux-musl --release \
    && mv target/${ARCHITECTURE}-unknown-linux-musl/release/graphgate target/graphgate

FROM scratch AS runner
WORKDIR /graphgate

COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY --from=builder /graphgate/target/graphgate ./

USER 1000
ENTRYPOINT [ "/graphgate/graphgate" ]
