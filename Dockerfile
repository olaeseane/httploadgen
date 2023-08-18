FROM rust:1.67 as builder
WORKDIR /usr/src/app
COPY . .
RUN RUSTFLAGS='-C target-feature=+crt-static' cargo install  --path client --bin agent

FROM gcr.io/distroless/base
# FROM debian:buster-slim
COPY --from=builder /usr/local/cargo/bin/agent /usr/local/bin/agent
CMD ["agent"]