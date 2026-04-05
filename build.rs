fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "windows" {
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("CompanyName", "koao");
        res.set("FileDescription", "Glass - Serial Monitor");
        res.set("ProductName", "Glass");
        res.set("LegalCopyright", "koao");
        res.compile().expect("リソースのコンパイルに失敗しました");
    }
}
