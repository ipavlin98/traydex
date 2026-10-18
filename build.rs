fn main() -> std::io::Result<()> {
    println!("cargo:rerun-if-changed=assets/traydex.ico");
    println!("cargo:rerun-if-changed=build.rs");
    let mut resource = winresource::WindowsResource::new();
    resource.set_icon("assets/traydex.ico");
    resource.set("ProductName", "Traydex");
    resource.set("ProductVersion", env!("CARGO_PKG_VERSION"));
    resource.set("FileVersion", env!("CARGO_PKG_VERSION"));
    resource.set("FileDescription", "Traydex");
    resource.set("LegalCopyright", "Traydex 2026");
    resource.compile()
}
