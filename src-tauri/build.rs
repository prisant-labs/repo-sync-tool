// src-tauri build script.
//
// Owning effort: E-01 (Foundation); E-12 (tracer bullet) adds the test-binary
// manifest embed below.
// Runs the Tauri build step that generates the context, embeds the config,
// and produces the permission/capability schemas consumed at runtime.

fn main() {
    // Embed a comctl32-v6 activation manifest into the crate's TEST binaries on
    // Windows + MSVC. The Tauri runtime imports comctl32 v6 symbols (e.g.
    // TaskDialogIndirect) that live only in the side-by-side v6 assembly. The
    // shipped reposync.exe receives a v6 manifest from tauri-build, but
    // `cargo test` executables do not, so the headless `export_bindings` test
    // would otherwise fail to start with STATUS_ENTRYPOINT_NOT_FOUND. This only
    // affects test binaries (`rustc-link-arg-tests`), never the real app.
    #[cfg(all(target_os = "windows", target_env = "msvc"))]
    {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tracer-test.manifest");
        println!("cargo::rerun-if-changed=tracer-test.manifest");
        // Scope the manifest to integration `[[test]]` binaries only via
        // `rustc-link-arg-tests`. The real reposync.exe must NOT get this arg:
        // tauri-build already embeds a MANIFEST resource into it, and a second
        // /MANIFESTINPUT triggers `CVT1100: duplicate resource type:MANIFEST`.
        // The headless bindings export therefore lives in `tests/`, not as a
        // `--lib` unit test (the lib test harness is not a `[[test]]` target,
        // so this arg would not reach it).
        println!("cargo::rustc-link-arg-tests=/MANIFEST:EMBED");
        println!(
            "cargo::rustc-link-arg-tests=/MANIFESTINPUT:{}",
            manifest.display()
        );
    }

    tauri_build::build()
}
