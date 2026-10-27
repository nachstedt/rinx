//! Writing the shell scripts tests run as a stand-in interpreter.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

/// Writes `text` to `path` as an executable script.
///
/// The file is written by a `/bin/sh` child rather than by this process:
/// while a test holds the file open for writing, another test thread's
/// fork inherits that descriptor until its exec, and running the script in
/// that window fails with `ETXTBSY` ("Text file busy"). A child's
/// descriptor is gone once it has exited.
pub(crate) fn write_executable_script(path: &Path, text: &str) {
    let mut writer = Command::new("/bin/sh")
        .args(["-c", r#"cat > "$0" && chmod 755 "$0""#])
        .arg(path)
        .stdin(Stdio::piped())
        .spawn()
        .expect("spawn /bin/sh");
    writer
        .stdin
        .take()
        .expect("a stdin pipe")
        .write_all(text.as_bytes())
        .expect("write the script");
    assert!(
        writer.wait().expect("wait").success(),
        "wrote {}",
        path.display()
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn test_write_executable_script_writes_a_runnable_file() {
        // Given
        let directory = std::env::temp_dir().join("rinx_lsp_test_scripts");
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("mkdir");
        let path = directory.join("script");

        // When
        write_executable_script(&path, "#!/bin/sh\necho ran\n");

        // Then
        let mode = std::fs::metadata(&path)
            .expect("exists")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o755);
        let output = Command::new(&path).output().expect("ran");
        assert_eq!(output.stdout, b"ran\n");
    }
}
