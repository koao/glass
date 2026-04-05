use std::path::Path;

fn main() {
    // Windowsリソース設定
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "windows" {
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("CompanyName", "koao");
        res.set("FileDescription", "Glass - Serial Monitor");
        res.set("ProductName", "Glass");
        res.set("LegalCopyright", "koao");
        res.compile().expect("リソースのコンパイルに失敗しました");
    }

    // protocols/ ディレクトリをビルド出力先にコピー
    copy_protocols_dir();
}

/// protocols/ ディレクトリをターゲットディレクトリにコピー
fn copy_protocols_dir() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let src_dir = Path::new(&manifest_dir).join("protocols");

    // protocols/ディレクトリが存在しない場合はスキップ
    if !src_dir.is_dir() {
        return;
    }

    // 変更検出
    println!("cargo:rerun-if-changed=protocols");

    // ターゲットディレクトリを取得
    // OUT_DIR は target/debug/build/<pkg>/out なので、3階層上がる
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let out_path = Path::new(&out_dir);
    // target/<profile>/ を取得
    let target_dir = out_path
        .ancestors()
        .nth(3)
        .expect("ターゲットディレクトリの解決に失敗");

    let dst_dir = target_dir.join("protocols");
    let _ = std::fs::create_dir_all(&dst_dir);

    // .toml ファイルをコピー
    if let Ok(entries) = std::fs::read_dir(&src_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("toml") {
                let dst = dst_dir.join(entry.file_name());
                let _ = std::fs::copy(&path, &dst);
            }
        }
    }
}
