all: build
VERSION=1.0.0

build:
	cargo build --release --target x86_64-unknown-linux-musl

docker:
	docker build -t livewin_live:latest .

docker-build: noCgoBuild docker

deploy:
	scp -P 52922 target/x86_64-unknown-linux-musl/release/xlive wida@home.wida.cool:/data/app/xlive/
	scp -P 52922 conf.yaml wida@home.wida.cool:/data/app/xlive/
clean:
	rm -rf target
