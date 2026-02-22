#!/usr/bin/env bash
# Build busybox bootstrap archive for Android aarch64.
# Requires: ANDROID_NDK_HOME, git, make
set -euo pipefail

BUSYBOX_VERSION="${BUSYBOX_VERSION:-1.36.1}"
BUSYBOX_URL="https://busybox.net/downloads/busybox-${BUSYBOX_VERSION}.tar.bz2"
API_LEVEL=26

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ANDROID_DIR="$(dirname "$SCRIPT_DIR")"
ASSETS_DIR="${ANDROID_DIR}/app/src/main/assets"
BUILD_DIR="${SCRIPT_DIR}/.build"
STAGING_DIR="${BUILD_DIR}/staging"

if [ -z "${ANDROID_NDK_HOME:-}" ]; then
    echo "Error: ANDROID_NDK_HOME is not set"
    exit 1
fi

# Find NDK toolchain
TOOLCHAIN="${ANDROID_NDK_HOME}/toolchains/llvm/prebuilt/linux-x86_64"
if [ ! -d "$TOOLCHAIN" ]; then
    echo "Error: NDK toolchain not found at ${TOOLCHAIN}"
    exit 1
fi

CC="${TOOLCHAIN}/bin/aarch64-linux-android${API_LEVEL}-clang"
CROSS_PREFIX="${TOOLCHAIN}/bin/llvm-"

echo "==> Downloading busybox ${BUSYBOX_VERSION}..."
mkdir -p "$BUILD_DIR"
cd "$BUILD_DIR"

if [ ! -d "busybox-${BUSYBOX_VERSION}" ]; then
    curl -fsSL "$BUSYBOX_URL" | tar xj
fi

cd "busybox-${BUSYBOX_VERSION}"

echo "==> Configuring busybox..."
make defconfig

# Enable ash prompt expansion for PS1 escapes
sed -i 's/# CONFIG_ASH_EXPAND_PRMT is not set/CONFIG_ASH_EXPAND_PRMT=y/' .config

# Force static build
sed -i 's/# CONFIG_STATIC is not set/CONFIG_STATIC=y/' .config
# Disable features that won't work on Android
sed -i 's/CONFIG_FEATURE_HAVE_RPC=y/# CONFIG_FEATURE_HAVE_RPC is not set/' .config
sed -i 's/CONFIG_FEATURE_INETD_RPC=y/# CONFIG_FEATURE_INETD_RPC is not set/' .config
sed -i 's/CONFIG_FEATURE_UTMP=y/# CONFIG_FEATURE_UTMP is not set/' .config
sed -i 's/CONFIG_FEATURE_WTMP=y/# CONFIG_FEATURE_WTMP is not set/' .config

echo "==> Building busybox (static, aarch64)..."
make -j"$(nproc)" \
    CC="$CC" \
    CROSS_COMPILE="$CROSS_PREFIX" \
    LDFLAGS="--static" \
    CONFIG_STATIC=y

echo "==> Staging archive..."
rm -rf "$STAGING_DIR"
mkdir -p "$STAGING_DIR/usr/bin"

cp busybox "$STAGING_DIR/usr/bin/busybox"

# Create setup-storage helper script
cat > "$STAGING_DIR/usr/bin/setup-storage" << 'SETUP_STORAGE'
#!/usr/bin/env sh
mkdir -p "$HOME/storage"
ln -sf /sdcard "$HOME/storage/shared"
ln -sf /sdcard/Download "$HOME/storage/downloads"
ln -sf /sdcard/DCIM "$HOME/storage/dcim"
ln -sf /sdcard/Pictures "$HOME/storage/pictures"
ln -sf /sdcard/Music "$HOME/storage/music"
ln -sf /sdcard/Movies "$HOME/storage/movies"
echo "Storage symlinks created in ~/storage/"
SETUP_STORAGE
chmod +x "$STAGING_DIR/usr/bin/setup-storage"

echo "==> Packaging bootstrap archive..."
mkdir -p "$ASSETS_DIR"
tar czf "$ASSETS_DIR/bootstrap-aarch64.tar.gz" -C "$STAGING_DIR" .

ARCHIVE_SIZE=$(du -h "$ASSETS_DIR/bootstrap-aarch64.tar.gz" | cut -f1)
echo "==> Done: ${ASSETS_DIR}/bootstrap-aarch64.tar.gz (${ARCHIVE_SIZE})"
