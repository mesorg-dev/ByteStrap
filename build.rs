fn main() {
    // Ресурсы (иконка) встраиваются только при сборке под Windows.
    // На хосте (Termux/Android) build.rs просто пропускает этот шаг.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/app.ico");
        if let Err(e) = res.compile() {
            eprintln!("warning: failed to embed resources: {e}");
        }
    }
}
