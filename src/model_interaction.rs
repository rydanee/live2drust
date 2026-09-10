use std::ffi::{CString, c_char};

#[link(name = "interaction", kind = "static")]
unsafe extern "C" {
    fn init(project_dir: *const c_char);
}

fn init_model() {
    let manifest_dir =
        std::env::var("CARGO_MANIFEST_DIR").expect("Failed to get CARGO_MANIFEST_DIR");
    let cman = CString::new(manifest_dir).expect("");

    unsafe {
        init(cman.as_ptr());
    }
}
