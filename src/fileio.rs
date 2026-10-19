//! Loading and saving files (always UTF-8 with LF line endings) and running shell commands.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::SystemTime;

use ropey::Rope;

pub struct Loaded {
    pub text: String,
    pub crlf: bool,
    pub lossy: bool,
    /// The file ended with a newline (which is stripped from `text`).
    pub final_newline: bool,
}

/// `Ok(None)` if the file does not exist (yet).
pub fn load(path: &Path) -> io::Result<Option<Loaded>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(decode(bytes))),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// Decodes raw file content: UTF-8 (lossy), no BOM, LF only, no final newline.
pub fn decode(bytes: Vec<u8>) -> Loaded {
    let (mut text, lossy) = match String::from_utf8(bytes) {
        Ok(s) => (s, false),
        Err(e) => (String::from_utf8_lossy(e.as_bytes()).into_owned(), true),
    };
    if text.starts_with('\u{feff}') {
        text.drain(..3);
    }
    let crlf = text.contains("\r\n");
    if crlf {
        text = text.replace("\r\n", "\n");
    }
    let final_newline = text.ends_with('\n');
    if final_newline {
        text.pop();
    }
    Loaded { text, crlf, lossy, final_newline }
}

fn write_rope(w: &mut impl Write, rope: &Rope) -> io::Result<()> {
    for chunk in rope.chunks() {
        w.write_all(chunk.as_bytes())?;
    }
    if rope.len_chars() > 0 {
        w.write_all(b"\n")?;
    }
    Ok(())
}

/// Modification time and size – used to notice changes made by other programs.
pub fn stamp(path: &Path) -> Option<(SystemTime, u64)> {
    let m = fs::metadata(path).ok()?;
    Some((m.modified().ok()?, m.len()))
}

/// Replaces the content of an already opened file in place.
fn overwrite(mut f: fs::File, rope: &Rope) -> io::Result<()> {
    f.set_len(0)?;
    let mut w = io::BufWriter::new(&mut f);
    write_rope(&mut w, rope)?;
    w.flush()?;
    drop(w);
    f.sync_all()
}

/// Writes to a temporary file next to the target and renames it afterwards,
/// so the target is never left half-written. Writes in place instead when the file
/// has hard links (renaming would break them), the directory is not writable, or the
/// temporary file cannot be given the owner and group of the original (e.g. `sudo nani ~/file`).
/// Fails with `PermissionDenied` for files we may not write – including read-only ones.
pub fn save(path: &Path, rope: &Rope) -> io::Result<()> {
    let target = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let existing = match fs::metadata(&target) {
        Ok(meta) => {
            // the rename below would silently replace a read-only file, so check first
            let file = fs::OpenOptions::new().write(true).open(&target)?;
            Some((meta, file))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => return Err(e),
    };
    #[cfg(unix)]
    if let Some((meta, _)) = &existing {
        use std::os::unix::fs::MetadataExt;
        if meta.nlink() > 1 {
            let (_, file) = existing.expect("checked above");
            return overwrite(file, rope);
        }
    }
    let dir = match target.parent() {
        Some(d) if !d.as_os_str().is_empty() => d.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let name = target
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid file name"))?;
    let tmp = dir.join(format!(".{}.nani-{}.tmp", name.to_string_lossy(), std::process::id()));
    let mut f = match fs::File::create(&tmp) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => match existing {
            Some((_, file)) => return overwrite(file, rope),
            None => return Err(e),
        },
        Err(e) => return Err(e),
    };
    #[cfg(unix)]
    if let Some((meta, _)) = &existing {
        use std::os::unix::fs::MetadataExt;
        let owned = |m: &fs::Metadata| (m.uid(), m.gid());
        let same = fs::metadata(&tmp).is_ok_and(|t| owned(&t) == owned(meta));
        if !same && std::os::unix::fs::chown(&tmp, Some(meta.uid()), Some(meta.gid())).is_err() {
            drop(f);
            let _ = fs::remove_file(&tmp);
            let (_, file) = existing.expect("checked above");
            return overwrite(file, rope);
        }
    }
    let res = (|| {
        let mut w = io::BufWriter::new(&mut f);
        write_rope(&mut w, rope)?;
        w.flush()?;
        drop(w);
        f.sync_all()?;
        if let Some((meta, _)) = &existing {
            fs::set_permissions(&tmp, meta.permissions())?;
        }
        fs::rename(&tmp, &target)
    })();
    if res.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    res
}

/// Saves through `sudo tee`. The terminal must be in normal mode so sudo can ask for a password.
#[cfg(unix)]
pub fn save_sudo(path: &Path, rope: &Rope) -> io::Result<()> {
    let mut child = Command::new("sudo")
        .arg("tee")
        .arg("--")
        .arg(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()?;
    let mut stdin = child.stdin.take().expect("piped stdin");
    write_rope(&mut stdin, rope)?;
    drop(stdin);
    if child.wait()?.success() {
        Ok(())
    } else {
        Err(io::Error::other("sudo failed"))
    }
}

#[cfg(not(unix))]
pub fn save_sudo(_path: &Path, _rope: &Rope) -> io::Result<()> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "sudo is not available on this system"))
}

/// Runs `cmd` in the system shell with `input` on stdin and returns its stdout.
pub fn pipe(cmd: &str, input: &str) -> Result<String, String> {
    #[cfg(windows)]
    let mut command = {
        use std::os::windows::process::CommandExt;
        // cmd.exe does not understand the quoting Rust applies to normal arguments
        // (`findstr "a b"` would arrive as `\"a b\"`), so pass the command line as is
        let mut c = Command::new("cmd");
        c.arg("/C").raw_arg(cmd);
        c
    };
    #[cfg(not(windows))]
    let mut command = {
        let mut c = Command::new("sh");
        c.args(["-c", cmd]);
        c
    };
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut stdin = child.stdin.take().expect("piped stdin");
    let data = input.to_owned();
    // write from a thread so a command that produces output early cannot deadlock us
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(data.as_bytes());
    });
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    let _ = writer.join();
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let first = err.lines().next().unwrap_or("").trim();
        return Err(if first.is_empty() { format!("Command failed ({})", out.status) } else { first.to_string() });
    }
    Ok(String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoding() {
        assert_eq!(decode(b"".to_vec()).text, "");
        assert_eq!(decode(b"a\r\nb\r\n".to_vec()).text, "a\nb");
        assert!(decode(b"a\r\nb".to_vec()).crlf);
        assert_eq!(decode("\u{feff}x\n\n".as_bytes().to_vec()).text, "x\n");
    }

    #[test]
    fn save_roundtrip() {
        let dir = std::env::temp_dir().join(format!("nani-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join("t.txt");
        fs::write(&p, "old").unwrap();
        save(&p, &Rope::from_str("new\nline")).unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "new\nline\n");
        save(&p, &Rope::from_str("")).unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn read_only_and_hard_links() {
        let dir = std::env::temp_dir().join(format!("nani-test-links-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let ro = dir.join("ro.txt");
        fs::write(&ro, "keep").unwrap();
        let mut perm = fs::metadata(&ro).unwrap().permissions();
        perm.set_readonly(true);
        fs::set_permissions(&ro, perm).unwrap();
        // root may write anyway – only check the error when we are not root
        if fs::OpenOptions::new().write(true).open(&ro).is_err() {
            let e = save(&ro, &Rope::from_str("new")).unwrap_err();
            assert_eq!(e.kind(), io::ErrorKind::PermissionDenied);
            assert_eq!(fs::read_to_string(&ro).unwrap(), "keep");
        }
        let a = dir.join("a.txt");
        let b = dir.join("b.txt");
        fs::write(&a, "old").unwrap();
        fs::hard_link(&a, &b).unwrap();
        save(&a, &Rope::from_str("new")).unwrap();
        assert_eq!(fs::read_to_string(&b).unwrap(), "new\n");
        let mut perm = fs::metadata(&ro).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        perm.set_readonly(false);
        fs::set_permissions(&ro, perm).unwrap();
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn piping() {
        assert_eq!(pipe("sort", "b\na\n").unwrap(), "a\nb\n");
        assert!(pipe("exit 3", "").is_err());
    }

    #[test]
    #[cfg(windows)]
    fn piping_keeps_quotes_on_windows() {
        assert_eq!(pipe(r#"echo "a b""#, "").unwrap().trim_end(), r#""a b""#);
        assert!(pipe("exit 3", "").is_err());
    }
}
