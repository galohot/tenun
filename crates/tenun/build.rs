// The web UI is embedded at compile time. A plain `cargo build` without the UI
// built first still works: it embeds a page that says how to build the UI.
fn main() {
    let dist = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ui/dist");
    if !dist.join("index.html").exists() {
        std::fs::create_dir_all(&dist).unwrap();
        std::fs::write(
            dist.join("index.html"),
            "<!doctype html><title>Tenun</title><p>The UI is not built. Run <code>pnpm -C ui build</code>, then rebuild.</p>",
        )
        .unwrap();
    }
    // The viewer bundle for exported dashboards (`pnpm -C ui build:viewer`).
    let viewer = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ui/dist-viewer");
    if !viewer.join("viewer.js").exists() {
        std::fs::create_dir_all(&viewer).unwrap();
        std::fs::write(
            viewer.join("viewer.js"),
            "document.body.textContent = 'The viewer is not built.';",
        )
        .unwrap();
        std::fs::write(viewer.join("viewer.css"), "").unwrap();
    }
    println!("cargo:rerun-if-changed=../../ui/dist");
    println!("cargo:rerun-if-changed=../../ui/dist-viewer");
}
