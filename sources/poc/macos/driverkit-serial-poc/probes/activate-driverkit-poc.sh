#!/bin/sh
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
poc_dir=$(CDPATH= cd -- "$script_dir/.." && pwd)
app="$poc_dir/xcode/build/DerivedData/Build/Products/Debug/WiremuxDriverKitSerialPOC.app"
dext="$app/Contents/Library/SystemExtensions/WiremuxSerialDriver.dext"
binary="$app/Contents/MacOS/WiremuxDriverKitSerialPOC"

printf 'wiremux DriverKit serial POC activation\n'
printf '=======================================\n'

if [ ! -x "$binary" ]; then
  printf 'error: app is not built at %s\n' "$app" >&2
  printf 'run: %s/build-driverkit-poc.sh\n' "$script_dir" >&2
  exit 1
fi

if [ ! -d "$dext" ]; then
  printf 'error: dext is not embedded at %s\n' "$dext" >&2
  exit 1
fi

if codesign --verify --strict --verbose=2 "$app" >/dev/null 2>&1; then
  printf 'ok: app signature verifies\n'
else
  printf 'warn: app signature does not verify for system-extension loading\n'
fi

if codesign --verify --strict --verbose=2 "$dext" >/dev/null 2>&1; then
  printf 'ok: dext signature verifies\n'
else
  printf 'warn: dext signature does not verify for system-extension loading\n'
fi

if command -v systemextensionsctl >/dev/null 2>&1; then
  printf '\nBefore activation:\n'
  systemextensionsctl list || true
fi

printf '\nSubmitting activation request through the host app...\n'
WIREMUX_DRIVERKIT_ACTIVATE=1 "$binary"

if command -v systemextensionsctl >/dev/null 2>&1; then
  printf '\nAfter activation:\n'
  systemextensionsctl list || true
fi

printf '\nCandidate serial nodes:\n'
ls -l /dev/tty.wiremux* /dev/cu.wiremux* 2>/dev/null || \
  printf 'none found\n'
