use std::env;
use std::process::Command;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    
    // Assemble boot.s
    let status = Command::new("clang")
        .args(&[
            "--target=aarch64-unknown-none",
            "-c",
            "src/boot.s",
            "-o",
            &format!("{}/boot.o", out_dir),
        ])
        .status();
    
    if status.is_err() || !status.unwrap().success() {
        Command::new("as")
            .args(&[
                "-arch", "arm64",
                "-o", &format!("{}/boot.o", out_dir),
                "src/boot.s"
            ])
            .status()
            .expect("Failed to assemble boot.s");
    }
    
    // Assemble vectors.s
    let status = Command::new("clang")
        .args(&[
            "--target=aarch64-unknown-none",
            "-c",
            "src/vectors.s",
            "-o",
            &format!("{}/vectors.o", out_dir),
        ])
        .status();
    
    if status.is_err() || !status.unwrap().success() {
        Command::new("as")
            .args(&[
                "-arch", "arm64",
                "-o", &format!("{}/vectors.o", out_dir),
                "src/vectors.s"
            ])
            .status()
            .expect("Failed to assemble vectors.s");
    }
    
    // Create archive with both object files
    Command::new("ar")
        .args(&["crus", &format!("{}/libboot.a", out_dir)])
        .arg(&format!("{}/boot.o", out_dir))
        .arg(&format!("{}/vectors.o", out_dir))
        .status()
        .expect("Failed to create archive");
    
    println!("cargo:rustc-link-search=native={}", out_dir);
    println!("cargo:rustc-link-lib=static=boot");
    println!("cargo:rerun-if-changed=src/boot.s");
    println!("cargo:rerun-if-changed=src/vectors.s");
    println!("cargo:rerun-if-changed=linker.ld");
}