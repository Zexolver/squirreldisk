fn main() {
    slint_build::compile("ui/app.slint").expect("failed to compile Slint UI");

    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("icons/icon.ico");
        let _ = res.compile();
    }
}
