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
SYSROOT="${TOOLCHAIN}/sysroot"

echo "==> Downloading busybox ${BUSYBOX_VERSION}..."
mkdir -p "$BUILD_DIR"
cd "$BUILD_DIR"

if [ ! -d "busybox-${BUSYBOX_VERSION}" ]; then
    curl -fsSL "$BUSYBOX_URL" | tar xj
fi

cd "busybox-${BUSYBOX_VERSION}"

# Clean any previous build
make clean 2>/dev/null || true

echo "==> Configuring busybox (android_ndk_defconfig)..."
# Use the bundled Android NDK config — disables bionic-incompatible features
make android_ndk_defconfig

# Override stale toolchain settings (config is from busybox 1.24/GCC era)
sed -i 's|^CONFIG_CROSS_COMPILER_PREFIX=.*|CONFIG_CROSS_COMPILER_PREFIX=""|' .config
sed -i "s|^CONFIG_SYSROOT=.*|CONFIG_SYSROOT=\"${SYSROOT}\"|" .config
sed -i 's|^CONFIG_EXTRA_CFLAGS=.*|CONFIG_EXTRA_CFLAGS="-DANDROID -D__ANDROID__ -fPIC"|' .config
sed -i 's|^CONFIG_EXTRA_LDFLAGS=.*|CONFIG_EXTRA_LDFLAGS=""|' .config
sed -i 's|^CONFIG_EXTRA_LDLIBS=.*|CONFIG_EXTRA_LDLIBS=""|' .config

# Enable static build
sed -i 's/# CONFIG_STATIC is not set/CONFIG_STATIC=y/' .config

# Enable ash shell with useful features
sed -i 's/# CONFIG_ASH is not set/CONFIG_ASH=y/' .config
sed -i 's/# CONFIG_ASH_BASH_COMPAT is not set/CONFIG_ASH_BASH_COMPAT=y/' .config
sed -i 's/# CONFIG_ASH_JOB_CONTROL is not set/CONFIG_ASH_JOB_CONTROL=y/' .config
sed -i 's/# CONFIG_ASH_ALIAS is not set/CONFIG_ASH_ALIAS=y/' .config
sed -i 's/# CONFIG_ASH_RANDOM_SUPPORT is not set/CONFIG_ASH_RANDOM_SUPPORT=y/' .config
sed -i 's/# CONFIG_ASH_EXPAND_PRMT is not set/CONFIG_ASH_EXPAND_PRMT=y/' .config
sed -i 's/# CONFIG_ASH_ECHO is not set/CONFIG_ASH_ECHO=y/' .config
sed -i 's/# CONFIG_ASH_PRINTF is not set/CONFIG_ASH_PRINTF=y/' .config
sed -i 's/# CONFIG_ASH_TEST is not set/CONFIG_ASH_TEST=y/' .config
sed -i 's/# CONFIG_ASH_GETOPTS is not set/CONFIG_ASH_GETOPTS=y/' .config
sed -i 's/# CONFIG_ASH_CMDCMD is not set/CONFIG_ASH_CMDCMD=y/' .config

# Disable features incompatible with Android bionic
sed -i 's/CONFIG_FEATURE_SYSLOG=y/# CONFIG_FEATURE_SYSLOG is not set/' .config
sed -i 's/CONFIG_TC=y/# CONFIG_TC is not set/' .config
sed -i 's/CONFIG_SEEDRNG=y/# CONFIG_SEEDRNG is not set/' .config
sed -i 's/CONFIG_SWAPON=y/# CONFIG_SWAPON is not set/' .config
sed -i 's/CONFIG_SWAPOFF=y/# CONFIG_SWAPOFF is not set/' .config

# Resolve any new config options from version drift (1.24 -> 1.36)
# yes exits with SIGPIPE when make closes stdin — ignore it
set +o pipefail
yes "" | make oldconfig CC="$CC" HOSTCC=gcc
set -o pipefail

echo "==> Building busybox (static, aarch64)..."
make -j"$(nproc)" \
    CC="$CC" \
    HOSTCC=gcc \
    STRIP="${TOOLCHAIN}/bin/llvm-strip" \
    LDFLAGS="-static -Wl,--allow-multiple-definition"

echo "==> Staging archive..."
rm -rf "$STAGING_DIR"
mkdir -p "$STAGING_DIR/usr/bin"
mkdir -p "$STAGING_DIR/usr/share/terminfo"

cp busybox "$STAGING_DIR/usr/bin/busybox"

# Compile terminfo database
TERMINFO_SRC="${ANDROID_DIR}/../../misc/omni-terminal.terminfo"
if [ -f "$TERMINFO_SRC" ]; then
    echo "==> Compiling terminfo..."
    tic -o "$STAGING_DIR/usr/share/terminfo" "$TERMINFO_SRC"
else
    echo "Warning: terminfo source not found at ${TERMINFO_SRC}"
fi

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

# Download Termux's proot + libtalloc (has SIGSYS handler for Android's Zygote seccomp filter)
PROOT_DEB_URL="https://packages.termux.dev/apt/termux-main/pool/main/p/proot/proot_5.1.107-70_aarch64.deb"
TALLOC_DEB_URL="https://packages.termux.dev/apt/termux-main/pool/main/libt/libtalloc/libtalloc_2.4.3_aarch64.deb"
DEB_EXTRACT_DIR="${BUILD_DIR}/deb-extract"

echo "==> Downloading Termux proot..."
mkdir -p "$DEB_EXTRACT_DIR"
curl -fsSL -o "$DEB_EXTRACT_DIR/proot.deb" "$PROOT_DEB_URL"
curl -fsSL -o "$DEB_EXTRACT_DIR/libtalloc.deb" "$TALLOC_DEB_URL"

# Extract binaries from .deb packages (ar + tar, no dpkg needed)
mkdir -p "$DEB_EXTRACT_DIR/proot" "$DEB_EXTRACT_DIR/talloc"
ar x "$DEB_EXTRACT_DIR/proot.deb" --output="$DEB_EXTRACT_DIR/proot"
ar x "$DEB_EXTRACT_DIR/libtalloc.deb" --output="$DEB_EXTRACT_DIR/talloc"
tar xf "$DEB_EXTRACT_DIR/proot/data.tar.xz" -C "$DEB_EXTRACT_DIR/proot"
tar xf "$DEB_EXTRACT_DIR/talloc/data.tar.xz" -C "$DEB_EXTRACT_DIR/talloc"

cp "$DEB_EXTRACT_DIR/proot/data/data/com.termux/files/usr/bin/proot" "$STAGING_DIR/usr/bin/proot"
chmod +x "$STAGING_DIR/usr/bin/proot"

# Ship libtalloc and proot loader alongside proot
mkdir -p "$STAGING_DIR/usr/lib" "$STAGING_DIR/usr/libexec/proot"
cp "$DEB_EXTRACT_DIR/talloc/data/data/com.termux/files/usr/lib/libtalloc.so.2.4.3" "$STAGING_DIR/usr/lib/libtalloc.so"
cp "$DEB_EXTRACT_DIR/proot/data/data/com.termux/files/usr/libexec/proot/loader" "$STAGING_DIR/usr/libexec/proot/loader"
chmod +x "$STAGING_DIR/usr/libexec/proot/loader"

rm -rf "$DEB_EXTRACT_DIR"

echo "==> Packaging bootstrap archive..."
mkdir -p "$ASSETS_DIR"
tar czf "$ASSETS_DIR/bootstrap-aarch64.tar.gz" -C "$STAGING_DIR" .

ARCHIVE_SIZE=$(du -h "$ASSETS_DIR/bootstrap-aarch64.tar.gz" | cut -f1)
echo "==> Done: ${ASSETS_DIR}/bootstrap-aarch64.tar.gz (${ARCHIVE_SIZE})"
