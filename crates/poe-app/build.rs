fn main() {
    #[cfg(windows)]
    {
        let manifest_dir = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
        let icon = std::fs::canonicalize(manifest_dir.join("../../packaging/windows/portal.ico"))
            .expect("portal.ico missing");
        println!("cargo:rerun-if-changed={}", icon.display());
        let mut res = winres::WindowsResource::new();
        res.set_icon(icon.to_str().expect("portal.ico path must be utf-8"));
        res.set("ProductName", "RustyExile");
        res.set("FileDescription", "Path of Exile 1 campaign leveling overlay");
        if let Err(err) = res.compile() {
            panic!("winres failed: {err}");
        }
    }
}
