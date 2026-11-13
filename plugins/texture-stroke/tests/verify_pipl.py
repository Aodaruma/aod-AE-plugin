"""Windows: compare compiled PiPL bytes with build.rs's binary input."""
import ctypes
import sys
from pathlib import Path


def verify(plugin, expected):
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.LoadLibraryExW.argtypes = [ctypes.c_wchar_p, ctypes.c_void_p, ctypes.c_uint]
    kernel.LoadLibraryExW.restype = ctypes.c_void_p
    kernel.FindResourceW.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_wchar_p]
    kernel.FindResourceW.restype = ctypes.c_void_p
    kernel.LoadResource.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
    kernel.LoadResource.restype = ctypes.c_void_p
    kernel.LockResource.argtypes = [ctypes.c_void_p]
    kernel.LockResource.restype = ctypes.c_void_p
    kernel.SizeofResource.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
    kernel.FreeLibrary.argtypes = [ctypes.c_void_p]
    module = kernel.LoadLibraryExW(str(plugin.resolve()), None, 2)  # data only
    if not module:
        raise ctypes.WinError(ctypes.get_last_error())
    try:
        resource = kernel.FindResourceW(module, 16000, "PiPL")
        if not resource:
            raise ctypes.WinError(ctypes.get_last_error())
        size = kernel.SizeofResource(module, resource)
        pointer = kernel.LockResource(kernel.LoadResource(module, resource))
        actual = ctypes.string_at(pointer, size)
        assert actual == expected.read_bytes(), "RC changed the binary PiPL payload"
        print(f"PASS compiled PiPL: {size} bytes match the binary input")
    finally:
        kernel.FreeLibrary(module)


if __name__ == "__main__":
    if len(sys.argv) != 3:
        raise SystemExit("Usage: python verify_pipl.py <plugin.dll/aex> <OUT_DIR/texture_stroke.pipl>")
    verify(Path(sys.argv[1]), Path(sys.argv[2]))
