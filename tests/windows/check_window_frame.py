"""Read the real Windows caption style, including for a window hidden in the tray."""

import argparse
import ctypes
import json
from ctypes import wintypes


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--pid", type=int, required=True)
    parser.add_argument("--expect-caption", action="store_true")
    args = parser.parse_args()
    user32 = ctypes.WinDLL("user32", use_last_error=True)
    callback_type = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
    user32.EnumWindows.argtypes = [callback_type, wintypes.LPARAM]
    user32.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
    user32.GetWindowTextW.argtypes = [wintypes.HWND, wintypes.LPWSTR, ctypes.c_int]
    user32.GetWindowLongPtrW.argtypes = [wintypes.HWND, ctypes.c_int]
    user32.GetWindowLongPtrW.restype = ctypes.c_ssize_t
    matches = []

    @callback_type
    def visit(handle, _):
        owner = wintypes.DWORD()
        user32.GetWindowThreadProcessId(handle, ctypes.byref(owner))
        if owner.value != args.pid:
            return True
        title = ctypes.create_unicode_buffer(256)
        user32.GetWindowTextW(handle, title, len(title))
        if title.value == "Codex Meter":
            style = user32.GetWindowLongPtrW(handle, -16)
            matches.append({"pid": args.pid, "nativeCaption": (style & 0x00C00000) == 0x00C00000, "style": hex(style)})
        return True

    if not user32.EnumWindows(visit, 0):
        raise ctypes.WinError(ctypes.get_last_error())
    if len(matches) != 1:
        raise SystemExit(f"Expected one Meter main window, found {len(matches)}")
    print(json.dumps(matches[0]))
    if matches[0]["nativeCaption"] != args.expect_caption:
        raise SystemExit("Unexpected native title bar state")


if __name__ == "__main__":
    main()
