fn main() {
    println!("cargo:rerun-if-changed=native/nfc_bridge.c");
    println!("cargo:rerun-if-env-changed=LIBNFC_INCLUDE_DIR");
    println!("cargo:rerun-if-env-changed=LIBNFC_LIB_DIR");

    let mut build = cc::Build::new();
    build.file("native/nfc_bridge.c").warnings(true);

    let libnfc_root = std::path::Path::new("../third_party/libnfc/build/install");
    let env_include = std::env::var("LIBNFC_INCLUDE_DIR").ok();
    let env_lib = std::env::var("LIBNFC_LIB_DIR").ok();

    if let (Some(include_dir), Some(lib_dir)) = (env_include, env_lib) {
        build.include(&include_dir);
        println!("cargo:rustc-link-search=native={lib_dir}");
        println!("cargo:rustc-link-lib=static=nfc");
        link_libusb();
    } else if libnfc_root.exists() {
        let include_dir = libnfc_root.join("include");
        let lib_dir = libnfc_root.join("lib");
        build.include(&include_dir);
        println!("cargo:rustc-link-search=native={}", lib_dir.display());
        println!("cargo:rustc-link-lib=static=nfc");
        link_libusb();
    } else {
        // Fallback to system libnfc.
        match pkg_config::Config::new().probe("libnfc") {
            Ok(lib) => {
                for include in lib.include_paths {
                    build.include(include);
                }
            }
            Err(_) => {
                for include in ["/opt/homebrew/include", "/usr/local/include"] {
                    if std::path::Path::new(include).exists() {
                        build.include(include);
                    }
                }
                for lib_path in ["/opt/homebrew/lib", "/usr/local/lib"] {
                    if std::path::Path::new(lib_path).exists() {
                        println!("cargo:rustc-link-search=native={lib_path}");
                    }
                }
                println!("cargo:rustc-link-lib=nfc");
            }
        }
    }

    build.compile("nfc_bridge");
    tauri_build::build();
}

fn link_libusb() {
    // libnfc static archive may reference libusb-compat symbols (usb_*).
    for pkg in ["libusb", "libusb-compat-0.1", "libusb-1.0"] {
        if pkg_config::Config::new().probe(pkg).is_ok() {
            return;
        }
    }
    // Fallback for environments without pkg-config metadata.
    println!("cargo:rustc-link-lib=usb");
}
