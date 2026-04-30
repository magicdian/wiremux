#!/bin/sh
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
poc_dir=$(CDPATH= cd -- "$script_dir/.." && pwd)
xcode_dir="$poc_dir/xcode"
project="$xcode_dir/WiremuxDriverKitSerialPOC.xcodeproj"
derived_data="$xcode_dir/build/DerivedData"

printf 'wiremux DriverKit serial POC build\n'
printf '===================================\n'

if [ ! -d "$project" ]; then
  printf 'error: missing Xcode project: %s\n' "$project" >&2
  exit 1
fi

code_signing_allowed=${CODE_SIGNING_ALLOWED:-NO}

set -- xcodebuild \
  -project "$project" \
  -scheme WiremuxDriverKitSerialPOC \
  -configuration Debug \
  -derivedDataPath "$derived_data" \
  CODE_SIGNING_ALLOWED="$code_signing_allowed" \
  build

if [ -n "${DEVELOPMENT_TEAM:-}" ]; then
  set -- "$@" DEVELOPMENT_TEAM="$DEVELOPMENT_TEAM"
fi

if [ -n "${CODE_SIGN_IDENTITY:-}" ]; then
  set -- "$@" CODE_SIGN_IDENTITY="$CODE_SIGN_IDENTITY"
fi

if [ -n "${PROVISIONING_PROFILE_SPECIFIER:-}" ]; then
  set -- "$@" PROVISIONING_PROFILE_SPECIFIER="$PROVISIONING_PROFILE_SPECIFIER"
fi

"$@"
