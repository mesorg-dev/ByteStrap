fn main() {
    // Компиляция .slint-интерфейса (генерация Rust-кода в OUT_DIR).
    // slint-build — чистый Rust, работает на любом хосте.
    slint_build::compile("ui/app.slint").expect("failed to compile .slint UI");

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
