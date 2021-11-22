FROM alpine:3.15

# go path
ENV PATH=$PATH:/usr/local/go/bin

# rust path
ENV PATH=$PATH:/root/.cargo/bin

WORKDIR /app

RUN apk add alpine-sdk --update

# enable rust
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --default-toolchain nightly-2021-11-17

# enable go
COPY --from=golang:1.16-alpine /usr/local/go/ /usr/local/go/

COPY . .

RUN cargo build
