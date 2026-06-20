// src-tauri build script.
//
// Owning effort: E-01 (Foundation).
// Runs the Tauri build step that generates the context, embeds the config,
// and produces the permission/capability schemas consumed at runtime.

fn main() {
    tauri_build::build()
}
