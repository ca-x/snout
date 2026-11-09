fn main() {
    println!("cargo:rerun-if-changed=assets/snout.ico");
    #[cfg(windows)]
    winresource::WindowsResource::new()
        .set_icon("assets/snout.ico")
        .set("ProductName", "snout")
        .set("FileDescription", "snout Rime workbench")
        .compile()
        .expect("compile the Windows application icon");
}
