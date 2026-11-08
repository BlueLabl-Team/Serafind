//! Windows: puts the icon, the version information (publisher, description, version) and the
//! manifest into `serafind.exe`. Explorer shows them, and antivirus software trusts a program
//! that says what it is more than an anonymous one.

fn main() {
    // `cfg(windows)` here is the system that builds: the resource compiler of the Windows SDK
    // is only there. Building for Windows from Linux (to check the code) skips it.
    #[cfg(windows)]
    windows_resources();
}

#[cfg(windows)]
fn windows_resources() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut resources = winresource::WindowsResource::new();
    resources
        .set_icon("assets/logo/serafind.ico")
        .set("ProductName", "Serafind")
        .set("FileDescription", "Serafind - quick search bar")
        .set("CompanyName", "BlueLabL")
        .set("LegalCopyright", "Copyright (c) BlueLabL. Apache License 2.0.")
        .set("OriginalFilename", "serafind.exe")
        .set("InternalName", "serafind")
        .set_manifest(MANIFEST);
    resources.compile().expect("cannot add the Windows resources");
}

/// Runs with the rights of the user (no elevation prompt), on Windows 10 and 11.
#[cfg(windows)]
const MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" uiAccess="false"/>
      </requestedPrivileges>
    </security>
  </trustInfo>
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <application>
      <supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/>
    </application>
  </compatibility>
</assembly>
"#;
