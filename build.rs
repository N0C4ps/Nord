use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=nord.exe.manifest");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        let manifest = PathBuf::from(
            env::var_os("CARGO_MANIFEST_DIR").expect("Cargo sets the package directory"),
        )
        .join("nord.exe.manifest");
        println!("cargo:rustc-link-arg-bin=nord=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg-bin=nord=/MANIFESTINPUT:{}", manifest.display());
    }
}
