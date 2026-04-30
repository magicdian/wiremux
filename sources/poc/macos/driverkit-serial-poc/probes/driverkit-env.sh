#!/usr/bin/env bash
set -u

status=0

ok() {
  printf 'ok: %s\n' "$1"
}

warn() {
  printf 'warn: %s\n' "$1"
}

fail() {
  printf 'fail: %s\n' "$1"
  status=1
}

command_text() {
  if "$@" >/tmp/wiremux-driverkit-probe.out 2>/tmp/wiremux-driverkit-probe.err; then
    cat /tmp/wiremux-driverkit-probe.out
    return 0
  fi
  cat /tmp/wiremux-driverkit-probe.err
  return 1
}

printf 'wiremux DriverKit environment probe\n'
printf '===================================\n'

if ! command -v xcode-select >/dev/null 2>&1; then
  fail "xcode-select is not available"
else
  developer_dir="$(command_text xcode-select -p)"
  if [ -n "$developer_dir" ] && [ -d "$developer_dir" ]; then
    ok "Xcode developer directory: $developer_dir"
  else
    fail "xcode-select did not return a valid developer directory"
  fi
fi

if ! command -v xcrun >/dev/null 2>&1; then
  fail "xcrun is not available"
else
  sdk_path="$(command_text xcrun --sdk driverkit --show-sdk-path)"
  if [ -n "$sdk_path" ] && [ -d "$sdk_path" ]; then
    ok "DriverKit SDK: $sdk_path"
  else
    fail "DriverKit SDK was not found through xcrun --sdk driverkit"
  fi

  platform_path="$(command_text xcrun --sdk driverkit --show-sdk-platform-path)"
  if [ -n "$platform_path" ] && [ -d "$platform_path" ]; then
    ok "DriverKit platform: $platform_path"
  else
    fail "DriverKit platform path was not found"
  fi

  clang_version="$(command_text xcrun --sdk driverkit clang --version)"
  if [ -n "$clang_version" ]; then
    ok "DriverKit clang: $(printf '%s' "$clang_version" | head -n 1)"
  else
    fail "DriverKit clang is not available"
  fi
fi

if [ -n "${sdk_path:-}" ] && [ -d "$sdk_path" ]; then
  serial_framework="$sdk_path/System/DriverKit/System/Library/Frameworks/SerialDriverKit.framework"
  usb_serial_framework="$sdk_path/System/DriverKit/System/Library/Frameworks/USBSerialDriverKit.framework"
  io_user_serial="$serial_framework/Headers/IOUserSerial.iig"
  io_user_usb_serial="$usb_serial_framework/Headers/IOUserUSBSerial.iig"

  if [ -d "$serial_framework" ]; then
    ok "SerialDriverKit.framework exists"
  else
    fail "SerialDriverKit.framework missing"
  fi

  if [ -f "$io_user_serial" ]; then
    ok "IOUserSerial.iig exists"
  else
    fail "IOUserSerial.iig missing"
  fi

  if [ -d "$usb_serial_framework" ]; then
    ok "USBSerialDriverKit.framework exists"
  else
    warn "USBSerialDriverKit.framework missing; IOUserSerial POC may still proceed"
  fi

  if [ -f "$io_user_usb_serial" ]; then
    ok "IOUserUSBSerial.iig exists"
  else
    warn "IOUserUSBSerial.iig missing; avoid USB-specific POC base class"
  fi
fi

if command -v csrutil >/dev/null 2>&1; then
  csr_status="$(command_text csrutil status)"
  if printf '%s' "$csr_status" | grep -qi 'enabled'; then
    warn "$csr_status"
  else
    ok "$csr_status"
  fi
else
  warn "csrutil not available; cannot report SIP status"
fi

if command -v systemextensionsctl >/dev/null 2>&1; then
  developer_status="$(command_text systemextensionsctl developer)"
  if printf '%s' "$developer_status" | grep -qi 'cannot be used if System Integrity Protection is enabled'; then
    warn "systemextensionsctl developer is blocked while SIP is enabled"
  elif [ -n "$developer_status" ]; then
    ok "systemextensionsctl developer: $(printf '%s' "$developer_status" | tr '\n' ' ')"
  else
    warn "systemextensionsctl developer returned no output"
  fi
else
  warn "systemextensionsctl not available"
fi

if command -v security >/dev/null 2>&1; then
  identities="$(security find-identity -v -p codesigning 2>/dev/null)"
  identity_count="$(printf '%s\n' "$identities" | grep -c '^[[:space:]]*[0-9]')"
  if [ "$identity_count" -gt 0 ]; then
    ok "codesigning identities visible: $identity_count"
  else
    warn "no codesigning identities visible"
  fi
else
  warn "security command not available; cannot inspect signing identities"
fi

rm -f /tmp/wiremux-driverkit-probe.out /tmp/wiremux-driverkit-probe.err

if [ "$status" -eq 0 ]; then
  printf 'result: DriverKit SDK probe passed; loading may still require signing and entitlements.\n'
else
  printf 'result: DriverKit SDK probe failed; fix the failed checks above before building a dext.\n'
fi

exit "$status"
