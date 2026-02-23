misc_dir := "misc"
target := "omni-terminal"
target_dir := "target/release"
target_dir_debian := "target/debian"
target_dir_osx := target_dir + "/osx"
release_dir := "release"
app_name := "OmniTerminal.app"
app_template := misc_dir + "/osx/" + app_name
app_binary := target_dir + "/" + target
app_binary_dir := target_dir_osx + "/" + app_name + "/Contents/MacOS"
app_extras_dir := target_dir_osx + "/" + app_name + "/Contents/Resources"
terminfo := misc_dir + "/omni-terminal.terminfo"
mandir := "/usr/local/share/man"

# Build universal macOS binary (x86_64 + arm64 via lipo)
universal:
    RUSTFLAGS='-C link-arg=-s' MACOSX_DEPLOYMENT_TARGET="10.15" cargo build --release --target=x86_64-apple-darwin
    RUSTFLAGS='-C link-arg=-s' MACOSX_DEPLOYMENT_TARGET="11.0" cargo build --release --target=aarch64-apple-darwin
    lipo target/{x86_64,aarch64}-apple-darwin/release/{{target}} -create -output {{app_binary}}

# Bundle universal binary into OmniTerminal.app
app-universal: universal
    mkdir -p {{app_binary_dir}}
    mkdir -p {{app_extras_dir}}
    cp -fRp {{app_template}} {{target_dir_osx}}
    cp -fp {{app_binary}} {{app_binary_dir}}
    touch -r "{{app_binary}}" "{{target_dir_osx}}/{{app_name}}"

# Install terminfo entries into the app bundle
install-terminfo:
    tic -xe xterm-omni-terminal,omni-terminal -o {{app_extras_dir}} {{terminfo}}

# Create unsigned macOS release zip
release-macos: app-universal
    codesign --remove-signature "{{target_dir_osx}}/{{app_name}}"
    codesign --force --deep --sign - "{{target_dir_osx}}/{{app_name}}"
    mkdir -p {{release_dir}}
    cp -rf ./target/release/osx/* ./{{release_dir}}/
    cd ./{{release_dir}} && zip -r ./macos-unsigned.zip ./*

# Create signed and notarized macOS release (requires cargo: `just release-macos-signed 0.1.0`)
release-macos-signed version: install-terminfo app-universal
    codesign --force --deep --options runtime --sign "Developer ID Application: Omni LLC" "{{target_dir_osx}}/{{app_name}}"
    mkdir -p {{release_dir}}
    cp -rf ./target/release/osx/* ./{{release_dir}}/
    ditto -c -k --keepParent ./{{release_dir}}/{{app_name}} ./{{release_dir}}/OmniTerminal-v{{version}}.zip
    xcrun notarytool submit ./{{release_dir}}/OmniTerminal-v{{version}}.zip --keychain-profile "Omni LLC" --wait
    rm -rf ./{{release_dir}}/{{app_name}}
    unzip ./{{release_dir}}/OmniTerminal-v{{version}}.zip -d ./{{release_dir}}
    @echo "Verify that 'OmniTerminal.app/Contents/Resources/' exists before running release-macos-dmg"

# Create macOS DMG (run after release-macos-signed)
release-macos-dmg version:
    cd ./{{release_dir}} && create-dmg {{app_name}} --dmg-title="Omni Terminal {{version}}" --overwrite

# Install OmniTerminal.app to /Applications
install-macos: release-macos
    rm -rf /Applications/{{app_name}}
    mv ./{{release_dir}}/{{app_name}} /Applications/

# Update version strings across the project (e.g. `just update-version 0.1.13 0.1.14`)
update-version old new:
    sed -i '' 's/{{old}}/{{new}}/g' Cargo.toml
    sed -i '' 's/Unreleased/Unreleased\n\n- TBD\n\n## {{new}}/g' CHANGELOG.md
    sed -i '' 's/{{old}}/{{new}}/g' {{misc_dir}}/windows/omni-terminal.wxs
    sed -i '' 's/{{old}}/{{new}}/g' {{app_template}}/Contents/Info.plist

# Build and package Debian x11 release (requires cargo-deb)
release-debian-x11:
    cargo deb -p omni-terminal -- --no-default-features --features=x11
    mkdir -p {{release_dir}}/debian/x11
    mv {{target_dir_debian}}/* {{release_dir}}/debian/x11/
    cd {{release_dir}}/debian/x11 && rename 's/.deb/_x11.deb/g' *

# Build and package Debian Wayland release (requires cargo-deb)
release-debian-wayland:
    cargo deb -p omni-terminal -- --no-default-features --features=wayland
    mkdir -p {{release_dir}}/debian/wayland
    mv {{target_dir_debian}}/* {{release_dir}}/debian/wayland/
    cd {{release_dir}}/debian/wayland && rename 's/.deb/_wayland.deb/g' *

# Build and install Debian x11 package
install-debian-x11:
    cargo install cargo-deb
    cargo deb -p omni-terminal --install -- --release --no-default-features --features=x11

# Build and install Debian Wayland package
install-debian-wayland:
    cargo install cargo-deb
    cargo deb -p omni-terminal --install -- --release --no-default-features --features=wayland

# Build Windows installer (requires cargo-wix)
release-windows:
    cargo wix -p omni-terminal

# Build npm package (@omnidotdev/terminal)
wasm-pack:
    cd frontends/wasm && bun run build

# Install wasm frontend dependencies (Binaryen/wasm-opt also required: https://github.com/WebAssembly/binaryen)
wasm-install:
    cargo install cargo-server
    cargo install wasm-bindgen-cli
    cargo install cargo-watch

# Build wasm frontend (debug)
wasm-build:
    cargo build -p omni-terminal-wasm --target wasm32-unknown-unknown
    ~/.cargo/bin/wasm-bindgen target/wasm32-unknown-unknown/debug/omni_terminal_wasm.wasm --out-dir frontends/wasm/wasm --target web --no-typescript

# Build wasm frontend (release)
wasm-build-release:
    cargo build -p omni-terminal-wasm --target wasm32-unknown-unknown --release
    ~/.cargo/bin/wasm-bindgen target/wasm32-unknown-unknown/release/omni_terminal_wasm.wasm --out-dir frontends/wasm/wasm --target web --no-typescript

# Optimize wasm binary size
wasm-opt:
    du -h frontends/wasm/wasm/omni_terminal_wasm_bg.wasm
    wasm-opt -O frontends/wasm/wasm/omni_terminal_wasm_bg.wasm -o frontends/wasm/wasm/omni_terminal_wasm_bg.wasm
    du -h frontends/wasm/wasm/omni_terminal_wasm_bg.wasm

# Build and serve wasm frontend (static, no PTY)
wasm-run: wasm-build
    cd frontends/wasm && cargo server --open

# Build and serve web terminal (WASM + WebSocket PTY server)
web-serve: wasm-build
    cargo run -p web-server

# Watch and rebuild wasm frontend on changes
wasm-watch:
    cargo watch -- just wasm-build

# Install sugarloaf wasm dependencies
sugarloaf-install:
    cargo install cargo-server
    cargo install wasm-bindgen-cli

# Run sugarloaf text example
sugarloaf-dev:
    cargo run --example text

# Build sugarloaf wasm
sugarloaf-build:
    cargo build -p sugarloaf-wasm --target wasm32-unknown-unknown
    ~/.cargo/bin/wasm-bindgen target/wasm32-unknown-unknown/debug/sugarloaf_wasm.wasm --out-dir sugarloaf/wasm --target web --no-typescript

# Serve sugarloaf wasm
sugarloaf-run: sugarloaf-build
    cd sugarloaf && cargo server

# Run sugarloaf wasm tests with Chrome
sugarloaf-test:
    GECKODRIVER=chromedriver cargo test -p sugarloaf --tests --target wasm32-unknown-unknown

# Run sugarloaf wasm tests with Firefox (requires cargo install geckodriver)
sugarloaf-test-firefox:
    GECKODRIVER=geckodriver cargo test -p sugarloaf --tests --target wasm32-unknown-unknown

# Build bootstrap archive (busybox + proot) for Android aarch64
android-bootstrap:
    frontends/android/scripts/build-bootstrap.sh

# Build native library for Android arm64
android-native:
    cargo ndk -t aarch64-linux-android build -p omni-terminal-android
    mkdir -p frontends/android/app/src/main/jniLibs/arm64-v8a
    cp target/aarch64-linux-android/debug/libomni_terminal_android.so frontends/android/app/src/main/jniLibs/arm64-v8a/

# Build native library for Android arm64 (release, optimized)
android-native-release:
    cargo ndk -t aarch64-linux-android build -p omni-terminal-android --release
    mkdir -p frontends/android/app/src/main/jniLibs/arm64-v8a
    cp target/aarch64-linux-android/release/libomni_terminal_android.so frontends/android/app/src/main/jniLibs/arm64-v8a/

# Build Android debug APK (native library + Kotlin shell)
android-build: android-native
    cd frontends/android && ./gradlew assembleDebug

# Build Android release APK
android-release: android-native-release
    cd frontends/android && ./gradlew assembleRelease

android_adb := env("ANDROID_ADB", "adb")
android_package := "dev.omnidotdev.terminal"
android_activity := android_package + "/.ConnectActivity"

# Build, install, and launch Android app on connected device
android-deploy: android-build
    {{android_adb}} install -r frontends/android/app/build/outputs/apk/debug/app-debug.apk
    {{android_adb}} shell am start -n {{android_activity}}

# Watch Rust sources, rebuild and deploy on change
android-watch:
    cargo watch -w frontends/android-lib/src -w sugarloaf/src -w copa/src -s 'just android-deploy'

# Clean Android build artifacts
android-clean:
    cd frontends/android && ./gradlew clean
    rm -rf frontends/android/app/src/main/jniLibs

# Build man pages from scdoc sources (requires scdoc)
man-pages:
    scdoc < extra/man/omni-terminal.1.scd > extra/man/omni-terminal.1
    scdoc < extra/man/omni-terminal.5.scd > extra/man/omni-terminal.5
    scdoc < extra/man/omni-terminal-bindings.5.scd > extra/man/omni-terminal-bindings.5

# Install man pages to system (requires sudo)
man-install: man-pages
    install -Dm644 extra/man/omni-terminal.1 {{mandir}}/man1/omni-terminal.1
    install -Dm644 extra/man/omni-terminal.5 {{mandir}}/man5/omni-terminal.5
    install -Dm644 extra/man/omni-terminal-bindings.5 {{mandir}}/man5/omni-terminal-bindings.5

# Remove built man pages
man-clean:
    rm -f extra/man/omni-terminal.1 extra/man/omni-terminal.5 extra/man/omni-terminal-bindings.5
