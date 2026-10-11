fn main() {
    // Embed the application manifest (common-controls v6, DPI awareness, asInvoker).
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("nutils.rc", embed_resource::NONE)
            .manifest_optional()
            .expect("embed manifest");
    }
    delay_load_prism_vendor_dlls();
    embed_commit();
}

/// Embed the commit this is built from as `NUTILS_COMMIT` (seven hex digits, or
/// empty outside a git checkout), for the dev update channel, which compares
/// commits. CI passes `GITHUB_SHA`; locally it comes from git.
fn embed_commit() {
    println!("cargo:rerun-if-env-changed=GITHUB_SHA");
    for f in [".git/HEAD", ".git/logs/HEAD"] {
        if std::path::Path::new(f).exists() {
            println!("cargo:rerun-if-changed={f}");
        }
    }
    let sha = std::env::var("GITHUB_SHA").ok().or_else(|| {
        let out = std::process::Command::new("git").args(["rev-parse", "HEAD"]).output().ok()?;
        out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
    });
    let short: String = sha.unwrap_or_default().chars().take(7).collect();
    println!("cargo:rustc-env=NUTILS_COMMIT={short}");
}

/// Delay-load the screen-reader DLLs Prism imports (`speech` feature, MSVC).
///
/// Prism's backends import vendor DLLs (Boy PC Reader, ZDSR, PC-Talker, …) that
/// most machines don't have. Delay-loaded, an import is resolved on first call
/// and Prism turns a missing DLL into "backend unavailable"; imported normally,
/// Windows refuses to start the process ("byctrl-x64.dll was not found").
///
/// Prism is linked statically and rustc performs the final link, and Cargo only
/// applies link args from the crate that emits them, so prismer can't set these
/// flags itself. It publishes the list instead, matching the backends that were
/// actually built.
fn delay_load_prism_vendor_dlls() {
    println!("cargo:rerun-if-env-changed=DEP_PRISMER_DELAY_LOAD_DLLS");
    if std::env::var_os("CARGO_FEATURE_SPEECH").is_none()
        || std::env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc")
    {
        return;
    }
    // Prism's Orca / speech-dispatcher bridges are Unix-only, so nothing here
    // imports them: naming them only earns an LNK4199 warning.
    const UNIX_ONLY: &[&str] = &["prism_orca_bridge.dll", "prism_speech_dispatcher_bridge.dll"];
    let dlls = std::env::var("DEP_PRISMER_DELAY_LOAD_DLLS").unwrap_or_default();
    for dll in dlls.split(';').filter(|dll| !dll.is_empty() && !UNIX_ONLY.contains(dll)) {
        println!("cargo:rustc-link-arg-bins=/DELAYLOAD:{dll}");
    }
    // Prism calls __FUnloadDelayLoadedDLL2 when a backend is torn down; without
    // this the unload is a no-op.
    println!("cargo:rustc-link-arg-bins=/DELAY:unload");
}
