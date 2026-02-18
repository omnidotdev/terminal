install:
	cargo install cargo-server
	cargo install wasm-bindgen-cli
	cargo install cargo-watch
	# Binaryen is also needed
	# https://github.com/WebAssembly/binaryen

run: build opt
	cargo server --open

opt:
	du -h ./wasm/omni-terminal_bg.wasm
	wasm-opt -O ./wasm/omni-terminal_bg.wasm -o ./wasm/omni-terminal_bg.wasm
	du -h ./wasm/omni-terminal_bg.wasm

watch:
	cargo watch -- make build

build:
	wasm-bindgen ../target/wasm32-unknown-unknown/debug/omni-terminal.wasm --out-dir wasm --target web --no-typescript

build-release:
	wasm-bindgen ../target/wasm32-unknown-unknown/release/omni-terminal.wasm --out-dir wasm --target web --no-typescript