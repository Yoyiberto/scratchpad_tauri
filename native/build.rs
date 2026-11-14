fn main() {
    // Icono del .exe (lo que muestra el buscador de Windows / Flow).
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../src-tauri/icons/icon.ico");
        if let Err(e) = res.compile() {
            println!("cargo:warning=no se pudo incrustar el icono: {e}");
        }
    }
}
