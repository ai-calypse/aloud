//! Text in an image, through the system's Vision framework.
//!
//! Vision is reached from JavaScript for Automation's Objective-C bridge, so
//! there is no Swift toolchain to ship or require: `osascript` is on every
//! Mac. The image path travels as one argv value, never inside the script.

use anyhow::Result;
use std::path::Path;

#[cfg(target_os = "macos")]
const SCRIPT: &str = r#"
ObjC.import('Vision');
ObjC.import('Foundation');
function run(argv) {
  const url = $.NSURL.fileURLWithPath(argv[0]);
  const handler = $.VNImageRequestHandler.alloc.initWithURLOptions(url, $({}));
  const request = $.VNRecognizeTextRequest.alloc.init;
  request.recognitionLevel = 0;
  request.usesLanguageCorrection = true;
  const ok = handler.performRequestsError($([request]), null);
  if (!ok) throw new Error('Vision could not read the image');
  const found = request.results;
  const lines = [];
  for (let i = 0; i < found.count; i++) {
    const top = found.objectAtIndex(i).topCandidates(1);
    if (top.count > 0) lines.push(ObjC.unwrap(top.objectAtIndex(0).string));
  }
  return lines.join('\n');
}
"#;

#[cfg(target_os = "macos")]
pub fn read(image: &Path) -> Result<String> {
    use anyhow::Context;
    let out = std::process::Command::new("osascript")
        .args(["-l", "JavaScript", "-e", SCRIPT, "--"])
        .arg(image)
        .output()
        .context("running osascript")?;
    anyhow::ensure!(
        out.status.success(),
        "text recognition failed: {}",
        String::from_utf8_lossy(&out.stderr).trim()
    );
    Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
}

#[cfg(not(target_os = "macos"))]
pub fn read(_image: &Path) -> Result<String> {
    anyhow::bail!("text recognition is macOS-only for now")
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    /// Renders real text into a PNG with `sips`-free pure code is not
    /// possible, so this only checks the failure path: a missing file is an
    /// error, not an empty string that looks like "no text".
    #[test]
    fn a_missing_image_is_an_error() {
        assert!(read(Path::new("/nonexistent/aloud.png")).is_err());
    }
}
