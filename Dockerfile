#######################################################################################################################
# BUILDER
#######################################################################################################################

FROM debian:bullseye-slim AS build

ENV PATH="/root/.cargo/bin/:$PATH"

RUN apt-get update && \
    apt-get install -y openssl libssl-dev gcc pkg-config curl && \
    curl https://sh.rustup.rs -sSf | sh -s -- --no-modify-path -y && \
    cargo install cargo-chef --locked && \
    cargo install --git https://github.com/polymny/template-config

WORKDIR /app

RUN cargo install --git https://github.com/polymny/template-config

COPY Cargo.toml .
COPY Cargo.lock .
RUN mkdir src && echo "fn main() { println!(\"Hello world\"); }" > src/main.rs
RUN cargo build --release

RUN rm -rf src
COPY src src

RUN cargo build --release

#######################################################################################################################
# SERVER
#######################################################################################################################
FROM debian:bullseye-slim AS server

WORKDIR /app

COPY --from=build /app/target/release/scraper /bin
COPY --from=build /root/.cargo/bin/template-config /bin

COPY static static
COPY templates templates
COPY Rocket.tpl.toml .

CMD template-config ./Rocket.tpl.toml > ./Rocket.toml && scraper serve
