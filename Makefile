
.PHONY: debug build-release release-linux-musl test clippy clippy-pedantic install install-debug

ARGS=-l
# ARGS=-l -d ~/code/extern/linux
# ARGS=-l -d ~/code/git-bare-test.git -w ~/code/git-bare-test

profile:
	cargo run --features=timing,pprof -- ${ARGS}

run-timing:
	cargo run --features=timing --release -- ${ARGS}

debug:
	RUST_BACKTRACE=true cargo run --features=timing -- ${ARGS}

build-release:
	cargo build --release

release-mac: build-release
	strip target/release/gitui
	otool -L target/release/gitui
	mkdir -p release
	tar -C ./target/release/ -czvf ./release/gitui-mac.tar.gz ./gitui
	ls -lisah ./release/gitui-mac.tar.gz

release-win: build-release
	mkdir -p release
	tar -C ./target/release/ -czvf ./release/gitui-win.tar.gz ./gitui.exe
	cargo install cargo-wix --version 0.3.3
	cargo wix -p gitui --no-build --nocapture --output ./release/gitui.msi
	ls -l ./release/gitui.msi 

release-linux-musl: build-linux-musl-release
	strip target/x86_64-unknown-linux-musl/release/gitui
	mkdir -p release
	tar -C ./target/x86_64-unknown-linux-musl/release/ -czvf ./release/gitui-linux-musl.tar.gz ./gitui

build-linux-musl-debug:
	cargo build --target=x86_64-unknown-linux-musl

build-linux-musl-release:
	cargo build --release --target=x86_64-unknown-linux-musl

test-linux-musl:
	cargo test --workspace --target=x86_64-unknown-linux-musl

release-linux-arm: build-linux-arm-release
	mkdir -p release

	aarch64-linux-gnu-strip target/aarch64-unknown-linux-gnu/release/gitui
	arm-linux-gnueabihf-strip target/armv7-unknown-linux-gnueabihf/release/gitui
	arm-linux-gnueabihf-strip target/arm-unknown-linux-gnueabihf/release/gitui

	tar -C ./target/aarch64-unknown-linux-gnu/release/ -czvf ./release/gitui-linux-aarch64.tar.gz ./gitui
	tar -C ./target/armv7-unknown-linux-gnueabihf/release/ -czvf ./release/gitui-linux-armv7.tar.gz ./gitui
	tar -C ./target/arm-unknown-linux-gnueabihf/release/ -czvf ./release/gitui-linux-arm.tar.gz ./gitui

build-linux-arm-debug:
	cargo build --target=aarch64-unknown-linux-gnu
	cargo build --target=armv7-unknown-linux-gnueabihf
	cargo build --target=arm-unknown-linux-gnueabihf

build-linux-arm-release:
	cargo build --release --target=aarch64-unknown-linux-gnu
	cargo build --release --target=armv7-unknown-linux-gnueabihf
	cargo build --release --target=arm-unknown-linux-gnueabihf

test:
	cargo test --workspace

fmt:
	cargo fmt -- --check

clippy:
	cargo clippy --workspace --all-features

clippy-nightly:
	cargo +nightly clippy --workspace --all-features

check: fmt clippy test deny

deny:
	cargo deny check

install:
	cargo install --path "." --offline --locked

install-timing:
	cargo install --features=timing --path "." --offline --locked

licenses:
	cargo bundle-licenses --format toml --output THIRDPARTY.toml

clean:
	cargo clean