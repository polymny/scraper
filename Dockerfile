#######################################################################################################################
# BUILDER
#######################################################################################################################
FROM nvidia/cuda:12.1.1-runtime-ubuntu22.04 AS build
WORKDIR /app
ENV PATH="/root/.cargo/bin/:$PATH"

# Install deps for rust build
RUN apt-get update && \
    apt-get install -y openssl libssl-dev gcc pkg-config curl && \
    rm -rf /var/lib/apt/lists/* && \
    curl https://sh.rustup.rs -sSf | sh -s -- --no-modify-path -y && \
    cargo install cargo-chef --locked && \
    cargo install --git https://github.com/polymny/template-config


RUN cargo install --git https://github.com/polymny/template-config
RUN cargo install ergol_cli

# Build rust dependencies
COPY Cargo.toml .
COPY Cargo.lock .
RUN mkdir src && echo "fn main() { println!(\"Hello world\"); }" > src/main.rs
RUN cargo build --release

# Build scraper
RUN rm -rf src
COPY src src
COPY migrations migrations
RUN cargo build --release

#######################################################################################################################
# SERVER
#######################################################################################################################
FROM nvidia/cuda:12.1.1-runtime-ubuntu22.04 AS server
WORKDIR /app

# Setup python
RUN apt-get update && apt-get install -y libssl3 python3 python3-pip python-is-python3 postgresql-client libgl1-mesa-glx libglib2.0-0 && rm -rf /var/lib/apt/lists/*
WORKDIR /app/python
COPY python/requirements.txt requirements.txt
COPY python/requirements_cuda.txt requirements_cuda.txt

RUN pip install --no-cache-dir -r requirements_cuda.txt
RUN pip install --no-cache-dir -r requirements.txt
COPY python/main.py main.py

# Setup rust
WORKDIR /app
COPY --from=build /app/target/release/scraper /bin
COPY --from=build /root/.cargo/bin/template-config /bin
COPY --from=build /root/.cargo/bin/ergol /bin

COPY static static
COPY templates templates
COPY --from=build /app/migrations migrations
COPY Rocket.tpl.toml .
COPY Cargo.toml .
COPY ./scripts/generate-examples.sh /usr/local/bin/generate-examples
COPY ./scripts/generate-csv.sh /usr/local/bin/generate-csv

# Generate Rocket.toml and start server
CMD template-config ./Rocket.tpl.toml > ./Rocket.toml && scraper serve
