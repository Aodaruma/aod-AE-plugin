"""Check embedded PiPL bytes for corruption by the Windows resource compiler."""
import ctypes as ct
from pathlib import Path
import struct
import sys


path = Path(sys.argv[1]).resolve()
kernel = ct.WinDLL("kernel32", use_last_error=True)
for name, args, result in [
    ("LoadLibraryExW", [ct.c_wchar_p, ct.c_void_p, ct.c_ulong], ct.c_void_p),
    ("FindResourceW", [ct.c_void_p, ct.c_void_p, ct.c_wchar_p], ct.c_void_p),
    ("LoadResource", [ct.c_void_p, ct.c_void_p], ct.c_void_p),
    ("LockResource", [ct.c_void_p], ct.c_void_p),
    ("SizeofResource", [ct.c_void_p, ct.c_void_p], ct.c_ulong),
    ("FreeLibrary", [ct.c_void_p], ct.c_int),
]:
    function = getattr(kernel, name)
    function.argtypes, function.restype = args, result

handle = kernel.LoadLibraryExW(str(path), None, 2)
assert handle, ct.get_last_error()
try:
    resource = kernel.FindResourceW(handle, 16000, "PiPL")
    assert resource, "PiPL resource missing"
    loaded = kernel.LoadResource(handle, resource)
    data = ct.string_at(kernel.LockResource(loaded), kernel.SizeofResource(handle, resource))
    assert data[:6] == b"\x01\0\0\0\0\0"
    count = struct.unpack_from("<I", data, 6)[0]
    offset, properties = 10, {}
    for _ in range(count):
        vendor, key, number, size = struct.unpack_from("<4s4sII", data, offset)
        assert vendor == b"MIB8" and number == 0 and size % 4 == 0
        offset += 16
        properties[key[::-1].decode("ascii")] = data[offset:offset + size]
        offset += size
    assert offset == len(data), "Property lengths no longer match the resource bytes"
    assert count == 13
    assert properties["name"][1:15] == b"AOD_ImageBevel"
    assert properties["eMNA"][1:11] == b"ImageBevel"
    assert struct.unpack("<I", properties["eGL2"])[0] == 0x08821488
    print("PASS: PiPL contains all 13 intact properties, including high-bit feature flags")
finally:
    kernel.FreeLibrary(handle)
