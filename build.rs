//! Build script: on Windows, embed an application manifest, icon, and version
//! metadata. Signed-looking, well-formed metadata reduces the odds that
//! SmartScreen / antivirus heuristics flag an unsigned freshly-built binary.

fn main() {
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon_with_id("assets/icon.ico", "1");
        res.set(
            "FileDescription",
            "CatchYT — YouTube audio & video downloader",
        );
        res.set("ProductName", "CatchYT");
        res.set("CompanyName", "CatchYT");
        res.set("LegalCopyright", "MIT-licensed. Uses yt-dlp and ffmpeg.");
        res.set("OriginalFilename", "catchyt.exe");
        res.set("FileVersion", env!("CARGO_PKG_VERSION"));
        res.set("ProductVersion", env!("CARGO_PKG_VERSION"));
        // A DPI-aware, modern-common-controls manifest.
        res.set_manifest(MANIFEST);
        if let Err(e) = res.compile() {
            // Do not fail the build if the icon is missing; just warn.
            println!("cargo:warning=winresource: {e}");
        }
    }
}

#[cfg(windows)]
const MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" uiAccess="false" />
      </requestedPrivileges>
    </security>
  </trustInfo>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>
    </windowsSettings>
  </application>
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls"
        version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*" />
    </dependentAssembly>
  </dependency>
</assembly>
"#;
