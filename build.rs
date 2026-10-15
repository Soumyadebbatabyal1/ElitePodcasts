use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let target = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo did not set OUT_DIR"))
        .join("icons.gresource");
    let result = Command::new("glib-compile-resources")
        .args(["--sourcedir=assets", "--target"])
        .arg(&target)
        .arg("assets/icons.gresource.xml")
        .status()
        .expect("glib-compile-resources is required to bundle the player icons");
    assert!(result.success(), "could not compile player icons");
    println!("cargo:rerun-if-changed=assets/icons.gresource.xml");
    for icon in [
        "skip-backwards-10-symbolic.svg",
        "skip-forward-10-symbolic.svg",
        "speedometer-symbolic.svg",
    ] {
        println!("cargo:rerun-if-changed=assets/icons/scalable/actions/{icon}");
    }
}
