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
