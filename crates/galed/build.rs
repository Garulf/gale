fn main() {
    println!("cargo:rerun-if-changed=../../assets/gale.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("../../assets/gale.ico")
            .compile()
            .expect("embed gale.ico into the galed executable");
    }
}
