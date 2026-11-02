fn main() {
    println!("cargo:rerun-if-env-changed=SHERPA_ONNX_LIB_DIR");
    if let Ok(root) = std::env::var("SHERPA_ONNX_LIB_DIR") {
        assert!(
            std::path::Path::new(&root).is_absolute(),
            "shared library directory must be absolute"
        );
        if cfg!(target_os = "macos") || cfg!(target_os = "linux") {
            println!("cargo:rustc-link-arg=-Wl,-rpath,{root}");
        }
    }
}
