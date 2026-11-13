use std::ffi::c_void;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleW(name: *const u16) -> *mut c_void;
    fn FindResourceW(module: *mut c_void, name: *const u16, kind: *const u16) -> *mut c_void;
    fn LoadResource(module: *mut c_void, resource: *mut c_void) -> *mut c_void;
    fn LockResource(resource: *mut c_void) -> *const u8;
    fn SizeofResource(module: *mut c_void, resource: *mut c_void) -> u32;
}

#[test]
fn linked_pipl_resource_matches_generated_bytes_exactly() {
    let expected = include_bytes!(concat!(env!("OUT_DIR"), "/seam-map.pipl"));
    let kind: Vec<u16> = "PiPL\0".encode_utf16().collect();
    // SAFETY: the current executable remains loaded, resource pointers are
    // checked before use, and the slice uses the resource's reported length.
    unsafe {
        let module = GetModuleHandleW(std::ptr::null());
        assert!(!module.is_null());
        let resource = FindResourceW(module, 16000usize as *const u16, kind.as_ptr());
        assert!(!resource.is_null(), "PiPL resource 16000 must be embedded");
        let data = LockResource(LoadResource(module, resource));
        assert!(!data.is_null());
        let actual = std::slice::from_raw_parts(data, SizeofResource(module, resource) as usize);
        assert_eq!(
            actual, expected,
            "Resource compiler changed the binary PiPL"
        );
    }
}
