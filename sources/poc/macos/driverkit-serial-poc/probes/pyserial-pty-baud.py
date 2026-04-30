#!/usr/bin/env python3
"""Probe pyserial custom-baud behavior on PTY-backed aliases."""

import argparse
import os
import pty
import sys


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Probe pyserial custom-baud behavior on a temporary PTY alias."
    )
    parser.add_argument(
        "--pause",
        action="store_true",
        help="pause after creating the PTY so another terminal can inspect it",
    )
    parser.add_argument(
        "--keep-alias",
        action="store_true",
        help="leave /tmp/wiremux-pty-baud-probe after exit for inspection",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        import serial
    except ImportError:
        print("fail: pyserial is not installed for this Python interpreter")
        return 1

    master_fd = None
    slave_fd = None
    alias_path = "/tmp/wiremux-pty-baud-probe"

    try:
        master_fd, slave_fd = pty.openpty()
        slave_path = os.ttyname(slave_fd)
        if os.path.lexists(alias_path):
            os.unlink(alias_path)
        os.symlink(slave_path, alias_path)

        print(f"pty real path: {slave_path}")
        print(f"pty alias path: {alias_path}")
        if args.pause:
            input("paused after PTY creation; press Enter to run the baud probe...")
        port = serial.Serial(alias_path, 115200, timeout=0.1)
        print(f"opened baud: {port.baudrate}")

        try:
            port.baudrate = 460800
            print(f"baud set ok: {port.baudrate}")
            print("result: PTY accepted the custom baud request on this platform")
            return 0
        except Exception as err:  # pyserial raises OSError on macOS PTY IOSSIOSPEED.
            print(f"baud set failed: {type(err).__name__}: {err}")
            print("result: PTY rejected the custom baud request; DriverKit remains relevant")
            return 0
        finally:
            port.close()
    finally:
        if os.path.lexists(alias_path) and not args.keep_alias:
            os.unlink(alias_path)
        if master_fd is not None:
            os.close(master_fd)
        if slave_fd is not None:
            os.close(slave_fd)


if __name__ == "__main__":
    sys.exit(main())
