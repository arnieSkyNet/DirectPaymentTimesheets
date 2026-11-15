// Included by the application and independently compiled by the exclusion test.
#[cfg(all(
    feature = "windows-dev-isolation",
    not(all(windows, debug_assertions, dpt_development_build))
))]
compile_error!(
    "windows-dev-isolation requires Windows and debug assertions; release builds are forbidden"
);
