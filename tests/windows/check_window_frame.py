"""Measure native title-bar space, including for a window hidden in the tray.

Tao keeps WS_CAPTION for frameless windows and suppresses the non-client area,
so the style bit alone cannot distinguish a visible native caption.
"""

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
    user32.SetProcessDpiAwarenessContext.argtypes = [ctypes.c_void_p]
    user32.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))

    class WindowInfo(ctypes.Structure):
        _fields_ = [("cbSize", wintypes.DWORD), ("rcWindow", wintypes.RECT),
                    ("rcClient", wintypes.RECT), ("dwStyle", wintypes.DWORD),
                    ("dwExStyle", wintypes.DWORD), ("dwWindowStatus", wintypes.DWORD),
                    ("cxWindowBorders", wintypes.UINT), ("cyWindowBorders", wintypes.UINT),
                    ("atomWindowType", wintypes.ATOM), ("wCreatorVersion", wintypes.WORD)]

    callback_type = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
    user32.EnumWindows.argtypes = [callback_type, wintypes.LPARAM]
    user32.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
    user32.GetWindowTextW.argtypes = [wintypes.HWND, wintypes.LPWSTR, ctypes.c_int]
    user32.GetWindowInfo.argtypes = [wintypes.HWND, ctypes.POINTER(WindowInfo)]
    user32.GetDpiForWindow.argtypes = [wintypes.HWND]
    user32.GetDpiForWindow.restype = wintypes.UINT
    user32.GetSystemMetricsForDpi.argtypes = [ctypes.c_int, wintypes.UINT]
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
            info = WindowInfo()
            info.cbSize = ctypes.sizeof(info)
            if not user32.GetWindowInfo(handle, ctypes.byref(info)):
                raise ctypes.WinError(ctypes.get_last_error())
            top_inset = info.rcClient.top - info.rcWindow.top
            caption_height = user32.GetSystemMetricsForDpi(4, user32.GetDpiForWindow(handle))
            matches.append({"pid": args.pid, "nativeCaption": top_inset >= caption_height,
                            "topInsetPixels": top_inset, "captionHeightPixels": caption_height})
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
