fn main() {
    // Make the UI build directory if it doesn't already exist.
    // We do this here because the tauri context macro expects it to
    // exist at build time, and it's otherwise manually required to create
    // it before building.
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    assert_eq!(manifest_dir.file_name().unwrap(), "gitbutler-tauri");
    let build_dir = manifest_dir
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("apps")
        .join("desktop")
        .join("build");
    if !build_dir.exists() {
        // NOTE(qix-): Do not use `create_dir_all` here - the parent directory
        // NOTE(qix-): already exists, and we want to fail if not (for some reason).
        #[expect(clippy::expect_fun_call, clippy::create_dir)]
        std::fs::create_dir(&build_dir).expect(
            format!("failed to create apps/desktop/build directory: {build_dir:?}").as_str(),
        );
    }
    let identifier = if let Ok(channel) = std::env::var("CHANNEL") {
        match channel.as_str() {
            "nightly" => "com.gitbutler.app.nightly",
            "release" => "com.gitbutler.app",
            _ => "com.gitbutler.app.dev",
        }
    } else {
        "com.gitbutler.app.dev"
    };
    println!("cargo:rustc-env=IDENTIFIER={identifier}");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        const STACK_SIZE_MIB: usize = 8;
        const MIB_IN_BYTES: usize = 1024 * 1024;
        let stack_size_bytes = STACK_SIZE_MIB * MIB_IN_BYTES;

        match std::env::var("CARGO_CFG_TARGET_ENV").as_deref() {
            Ok("msvc") => {
                println!(
                    "cargo:rustc-link-arg-bin=gitbutler-tauri=/STACK:{stack_size_bytes}"
                );
            }
            Ok("gnu") => {
                println!(
                    "cargo:rustc-link-arg-bin=gitbutler-tauri=-Wl,--stack,{stack_size_bytes}"
                );
            }
            _ => {}
        }
    }

    tauri_build::build();
}
