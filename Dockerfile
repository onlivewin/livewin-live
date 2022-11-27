FROM rust:1.58 as builder
WORKDIR /usr/src/xlive
COPY . .
#切换docker镜像到国内
COPY ./docker/config  /usr/local/cargo
RUN CARGO_HTTP_MULTIPLEXING=false cargo fetch && cargo install --path .

FROM debian:buster-slim
#RUN apt-get update && apt-get install -y extra-runtime-dependencies && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/cargo/bin/xlive /usr/local/bin/xlive
COPY conf.yaml /usr/src/xlive/conf.yaml
CMD ["xlive"]

# 本地编译拷贝
#FROM debian:buster-slim
#COPY ./target/release/xlive /usr/local/bin/xlive
#EXPOSE 1935
#EXPOSE 3000
#COPY conf.yaml /usr/src/xlive/conf.yaml
#CMD ["xlive"]