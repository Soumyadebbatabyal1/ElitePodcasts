use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo did not set OUT_DIR"));
    let target = out_dir.join("icons.gresource");
    let result = Command::new("glib-compile-resources")
        .args(["--sourcedir=assets", "--target"])
        .arg(&target)
        .arg("assets/icons.gresource.xml")
        .status()
        .expect("glib-compile-resources is required to bundle the player icons");
    assert!(result.success(), "could not compile player icons");
    println!("cargo:rerun-if-changed=assets/icons.gresource.xml");
    for icon in [
        "software-explore-symbolic.svg",
        "skip-backwards-10-symbolic.svg",
        "skip-forward-10-symbolic.svg",
        "speedometer-symbolic.svg",
    ] {
        println!("cargo:rerun-if-changed=assets/icons/scalable/actions/{icon}");
    }
    println!("cargo:rerun-if-changed=assets/icons/scalable/apps/io.github.elite.Podcasts.svg");
    println!("cargo:rerun-if-changed=assets/icons/scalable/apps/io.github.elite.Podcasts-symbolic.svg");
    let schemas = Command::new("glib-compile-schemas")
        .args(["--strict", "--targetdir"])
        .arg(&out_dir)
        .arg("data")
        .status()
        .expect("glib-compile-schemas is required to bundle application settings");
    assert!(schemas.success(), "could not compile application settings");
    println!("cargo:rerun-if-changed=data/io.github.elite.Podcasts.gschema.xml");
}
