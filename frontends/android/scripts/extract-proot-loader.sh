#!/usr/bin/env bash
# Extract the embedded ELF loader from a proot binary.
# proot embeds a small loader binary (used to bypass noexec mounts)
# as a raw blob referenced by _binary_loader_elf_{start,end} symbols.
set -euo pipefail

PROOT="${1:?Usage: $0 <proot-binary> <output-loader>}"
OUTPUT="${2:?Usage: $0 <proot-binary> <output-loader>}"

START_ADDR=$((0x$(nm "$PROOT" | awk '/_binary_loader_elf_start/{print $1}')))
END_ADDR=$((0x$(nm "$PROOT" | awk '/_binary_loader_elf_end/{print $1}')))
DATA_ADDR=$((0x$(readelf -S "$PROOT" | awk '/\.data /{print $4}')))
DATA_OFF=$((0x$(readelf -S "$PROOT" | awk '/\.data /{print $5}')))

FILE_OFF=$((DATA_OFF + START_ADDR - DATA_ADDR))
SIZE=$((END_ADDR - START_ADDR))

dd if="$PROOT" of="$OUTPUT" bs=1 skip=$FILE_OFF count=$SIZE 2>/dev/null
echo "Extracted proot loader ($SIZE bytes) to $OUTPUT"
