fn main() {
    // Fixed dark Fluent style so std widgets (the list scrollbar) match the
    // dark shell on every OS, regardless of the system light/dark setting.
    let config = slint_build::CompilerConfiguration::new().with_style("fluent-dark".into());
    slint_build::compile_with_config("ui/app-window.slint", config)
        .expect("failed to compile ui/app-window.slint");
}
