fn main() {
    if std::env::var("TARGET").unwrap().contains("windows") {
        // winresでダイアログリソースをコンパイル
        winres::WindowsResource::new()
            .set_resource_file("src/dialog.rc") // リソースファイルを指定
            .set_language(0x0411) // 日本語
            .compile()
            .unwrap();

        // リソースファイルの変更監視
        println!("cargo:rerun-if-changed=src/dialog.rc");
        println!("cargo:rerun-if-changed=src/resource.h");
        println!("cargo:rerun-if-changed=assets/images/camera_off.ico");
        println!("cargo:rerun-if-changed=assets/images/camera_on.ico");
        println!("cargo:rerun-if-changed=assets/images/select_area_off.ico");
        println!("cargo:rerun-if-changed=assets/images/select_area_on.ico");
    }
}
