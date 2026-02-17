.PHONY: docs

BUILD_MISC_DIR = misc
TARGET = omni-terminal
TARGET_DIR = target/release
TARGET_DIR_DEBIAN = target/debian
TARGET_DIR_OSX = $(TARGET_DIR)/osx
RELEASE_DIR = release

APP_NAME = OmniTerminal.app
APP_TEMPLATE = $(BUILD_MISC_DIR)/osx/$(APP_NAME)
APP_BINARY = $(TARGET_DIR)/$(TARGET)
APP_BINARY_DIR = $(TARGET_DIR_OSX)/$(APP_NAME)/Contents/MacOS
APP_EXTRAS_DIR = $(TARGET_DIR_OSX)/$(APP_NAME)/Contents/Resources
TERMINFO = $(BUILD_MISC_DIR)/omni-terminal.terminfo

all: install run

run:
	cargo run -p omni-terminal --release

# macOS: optionally you can run "/bin/launchctl setenv MTL_HUD_ENABLED 1"
dev:
	MTL_HUD_ENABLED=1 cargo run -p omni-terminal

dev-debug:
	MTL_HUD_ENABLED=1 RIO_LOG_LEVEL=debug make dev

dev-debug-wayland:
	RIO_LOG_LEVEL=debug cargo run -p omni-terminal --no-default-features --features=wayland

dev-debug-x11:
	RIO_LOG_LEVEL=debug cargo run -p omni-terminal --no-default-features --features=x11

run-wasm:
	cargo build -p omni-terminal --target wasm32-unknown-unknown --lib
	cargo run -p omni-terminal-wasm

dev-watch:
	#cargo install cargo-watch
	cargo watch -- cargo run -p omni-terminal

install:
	cargo fetch

build: install
	RUSTFLAGS='-C link-arg=-s' cargo build --release

$(TARGET)-universal:
	RUSTFLAGS='-C link-arg=-s' MACOSX_DEPLOYMENT_TARGET="10.15" cargo build --release --target=x86_64-apple-darwin
	RUSTFLAGS='-C link-arg=-s' MACOSX_DEPLOYMENT_TARGET="11.0" cargo build --release --target=aarch64-apple-darwin
	@lipo target/{x86_64,aarch64}-apple-darwin/release/$(TARGET) -create -output $(APP_BINARY)

app-universal: $(APP_NAME)-universal ## Create a universal OmniTerminal.app
$(APP_NAME)-%: $(TARGET)-%
	@mkdir -p $(APP_BINARY_DIR)
	@mkdir -p $(APP_EXTRAS_DIR)
	@cp -fRp $(APP_TEMPLATE) $(TARGET_DIR_OSX)
	@cp -fp $(APP_BINARY) $(APP_BINARY_DIR)
	@touch -r "$(APP_BINARY)" "$(TARGET_DIR_OSX)/$(APP_NAME)"

install-terminfo:
	@tic -xe xterm-omni-terminal,omni-terminal -o $(APP_EXTRAS_DIR) $(TERMINFO)

release-macos: app-universal
	@codesign --remove-signature "$(TARGET_DIR_OSX)/$(APP_NAME)"
	@codesign --force --deep --sign - "$(TARGET_DIR_OSX)/$(APP_NAME)"
	@echo "Created '$(APP_NAME)' in '$(TARGET_DIR_OSX)'"
	mkdir -p $(RELEASE_DIR)
	cp -rf ./target/release/osx/* ./release/
	cd ./release && zip -r ./macos-unsigned.zip ./*

release-macos-signed:
	$(eval VERSION = $(shell echo $(version)))
	$(if $(strip $(VERSION)),make release-macos-signed-app, make version-not-found)

release-macos-signed-app:
	@make install-terminfo
	@make app-universal
	@echo "Releasing Omni Terminal v$(version)"
	@codesign --force --deep --options runtime --sign "Developer ID Application: Omni LLC" "$(TARGET_DIR_OSX)/$(APP_NAME)"
	mkdir -p $(RELEASE_DIR) && cp -rf ./target/release/osx/* ./release/
	@ditto -c -k --keepParent ./release/$(APP_NAME) ./release/OmniTerminal-v$(version).zip
	@xcrun notarytool submit ./release/OmniTerminal-v$(version).zip --keychain-profile "Omni LLC" --wait
	rm -rf ./release/$(APP_NAME)
	@unzip ./release/OmniTerminal-v$(version).zip -d ./release
	@echo "Please verify if 'OmniTerminal.app/Contents/Resources/' exists before create-dmg"

install-macos: release-macos
	rm -rf /Applications/$(APP_NAME)
	mv ./release/$(APP_NAME) /Applications/

version-not-found:
	@echo "Omni Terminal version was not specified"
	@echo " - usage: $ make release-macos-signed version=0.0.0"

# e.g: make update-version old-version=0.1.13 new-version=0.1.12
update-version:
	@echo "Switching from $(old-version) to $(new-version)"
	find Cargo.toml -type f -exec sed -i '' 's/$(old-version)/$(new-version)/g' {} \;
	find CHANGELOG.md -type f -exec sed -i '' 's/Unreleased/Unreleased\n\n- TBD\n\n## $(new-version)/g' {} \;
	find $(BUILD_MISC_DIR)/windows/omni-terminal.wxs -type f -exec sed -i '' 's/$(old-version)/$(new-version)/g' {} \;
	find $(APP_TEMPLATE)/Contents/Info.plist -type f -exec sed -i '' 's/$(old-version)/$(new-version)/g' {} \;

release-macos-dmg:
#	Using https://www.npmjs.com/package/create-dmg
	cd ./release && create-dmg $(APP_NAME) --dmg-title="Omni Terminal ${version}" --overwrite

# TODO: Move to bin path
release-x11:
	RUSTFLAGS='-C link-arg=-s' cargo build --release --no-default-features --features=x11
	target/release/omni-terminal
release-wayland:
	RUSTFLAGS='-C link-arg=-s' cargo build --release --no-default-features --features=wayland
	target/release/omni-terminal

# Debian
# cargo install cargo-deb
# To install: sudo release/debian/omni-terminal_<version>_<architecture>_<feature>.deb
release-debian-x11:
	cargo deb -p omni-terminal -- --no-default-features --features=x11
	mkdir -p $(RELEASE_DIR)/debian/x11
	mv $(TARGET_DIR_DEBIAN)/* $(RELEASE_DIR)/debian/x11/
	cd $(RELEASE_DIR)/debian/x11 && rename 's/.deb/_x11.deb/g' *

release-debian-wayland:
	cargo deb -p omni-terminal -- --no-default-features --features=wayland
	mkdir -p $(RELEASE_DIR)/debian/wayland
	mv $(TARGET_DIR_DEBIAN)/* $(RELEASE_DIR)/debian/wayland/
	cd $(RELEASE_DIR)/debian/wayland && rename 's/.deb/_wayland.deb/g' *

# Release and Install
install-debian-x11:
	cargo install cargo-deb
	cargo deb -p omni-terminal --install -- --release --no-default-features --features=x11
install-debian-wayland:
	cargo install cargo-deb
	cargo deb -p omni-terminal --install -- --release --no-default-features --features=wayland

# cargo install cargo-wix
# https://github.com/volks73/cargo-wix
release-windows:
	cargo wix -p omni-terminal

lint:
	cargo fmt -- --check --color always
	cargo clippy --all-targets --all-features -- -D warnings

test:
	make lint
	RUST_BACKTRACE=full cargo test --release

publish-crates: build
	# Note: cargo publish is only supported from >=1.90
	cargo publish --workspace
