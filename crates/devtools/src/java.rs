//! Finding a JDK to start Gradle with, and downloading one when there is
//! none.
//!
//! The export's wrapper only needs Java 17 to start: its
//! `gradle-daemon-jvm.properties` makes Gradle fetch the Java 25 that Loom
//! needs for the daemon by itself. So any 17+ on the machine will do, and
//! when there is none, Stitchcraft downloads Eclipse Temurin 25 into its data
//! directory, which serves both purposes at once.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The oldest Java the Gradle wrapper starts on.
pub const MINIMUM: u32 = 17;
/// The Java Stitchcraft downloads, and what Minecraft 26.x builds with.
pub const DOWNLOAD: u32 = 25;

/// A Java installation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Java {
    pub home: PathBuf,
    pub version: u32,
}

impl Java {
    /// The `java` executable.
    pub fn binary(&self) -> PathBuf {
        binary_in(&self.home)
    }
}

fn binary_in(home: &Path) -> PathBuf {
    home.join("bin")
        .join(if cfg!(windows) { "java.exe" } else { "java" })
}

/// Where downloaded JDKs go.
pub fn downloads(data_dir: &Path) -> PathBuf {
    data_dir.join("java")
}

/// The best Java available: Stitchcraft's own download first (it is known to
/// be new enough for everything), then `JAVA_HOME`, then `java` on the
/// `PATH`. `None` when nothing is 17 or newer.
pub fn find(data_dir: &Path) -> Option<Java> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(downloads(data_dir)) {
        for entry in entries.flatten() {
            candidates.push(entry.path());
            candidates.push(entry.path().join("Contents/Home"));
        }
    }
    if let Some(home) = std::env::var_os("JAVA_HOME") {
        candidates.push(PathBuf::from(home));
    }
    let mut found: Vec<Java> = candidates
        .iter()
        .filter(|home| binary_in(home).is_file())
        .filter_map(|home| probe(&binary_in(home)))
        .collect();
    if found.is_empty() {
        found.extend(probe(Path::new("java")));
    }
    found
        .into_iter()
        .filter(|java| java.version >= MINIMUM)
        .max_by_key(|java| java.version)
}

/// Asks a `java` binary for its home and version.
fn probe(binary: &Path) -> Option<Java> {
    let output = Command::new(binary)
        .args(["-XshowSettings:properties", "-version"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stderr);
    parse_settings(&text)
}

/// Reads `java.home` and `java.specification.version` out of
/// `-XshowSettings:properties` output.
pub fn parse_settings(text: &str) -> Option<Java> {
    let property = |name: &str| {
        text.lines().find_map(|line| {
            let (key, value) = line.split_once('=')?;
            (key.trim() == name).then(|| value.trim().to_string())
        })
    };
    let home = property("java.home")?;
    let spec = property("java.specification.version")?;
    // "1.8" for Java 8, a bare number from 9 on.
    let version = spec
        .strip_prefix("1.")
        .unwrap_or(&spec)
        .parse()
        .ok()?;
    Some(Java {
        home: PathBuf::from(home),
        version,
    })
}

/// Adoptium's name for this OS and CPU, or `None` where it has no build.
fn platform() -> Option<(&'static str, &'static str)> {
    let os = match std::env::consts::OS {
        "linux" => "linux",
        "macos" => "mac",
        "windows" => "windows",
        _ => return None,
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "aarch64",
        _ => return None,
    };
    Some((os, arch))
}

/// The download URL for Temurin [`DOWNLOAD`] on this machine.
pub fn download_url() -> Option<String> {
    let (os, arch) = platform()?;
    Some(format!(
        "https://api.adoptium.net/v3/binary/latest/{DOWNLOAD}/ga/{os}/{arch}/jdk/hotspot/normal/eclipse"
    ))
}

/// Downloads and unpacks Temurin into [`downloads`], calling `progress` with
/// (bytes so far, total bytes or 0) as it goes.
pub fn download(data_dir: &Path, mut progress: impl FnMut(u64, u64)) -> Result<Java, String> {
    let url = download_url().ok_or("Stitchcraft can't download Java for this computer. Install a JDK 17 or newer and restart.")?;
    let agent = ureq::AgentBuilder::new().try_proxy_from_env(true).build();
    let response = agent
        .get(&url)
        .call()
        .map_err(|e| format!("Could not download Java: {e}"))?;
    let total: u64 = response
        .header("Content-Length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let mut reader = response.into_reader();
    let mut bytes = Vec::with_capacity(total as usize);
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|e| format!("Java download was cut off: {e}"))?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..read]);
        progress(bytes.len() as u64, total);
    }

    let root = downloads(data_dir);
    let staging = root.join(".unpacking");
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    if bytes.starts_with(b"PK") {
        zip::ZipArchive::new(std::io::Cursor::new(bytes))
            .and_then(|mut archive| archive.extract(&staging))
            .map_err(|e| format!("Could not unpack Java: {e}"))?;
    } else {
        tar::Archive::new(flate2::read::GzDecoder::new(&bytes[..]))
            .unpack(&staging)
            .map_err(|e| format!("Could not unpack Java: {e}"))?;
    }
    // The archive holds one top-level folder, like `jdk-25.0.1+8`.
    let top = std::fs::read_dir(&staging)
        .map_err(|e| e.to_string())?
        .flatten()
        .find(|entry| entry.path().is_dir())
        .ok_or("The Java download was empty.")?;
    let target = root.join(top.file_name());
    let _ = std::fs::remove_dir_all(&target);
    std::fs::rename(top.path(), &target).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(&staging);

    find(data_dir)
        .filter(|java| java.version >= DOWNLOAD)
        .ok_or_else(|| "Java was downloaded but does not start.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_are_read_for_old_and_new_numbering() {
        let new = "Property settings:\n    java.home = /opt/jdk-25\n    java.specification.version = 25\n";
        assert_eq!(
            parse_settings(new),
            Some(Java {
                home: PathBuf::from("/opt/jdk-25"),
                version: 25
            })
        );
        let old = "    java.home = C:\\Java\\jre8\n    java.specification.version = 1.8\n";
        assert_eq!(parse_settings(old).unwrap().version, 8);
        assert_eq!(parse_settings("nothing useful"), None);
    }

    #[test]
    fn there_is_a_download_for_common_machines() {
        if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
            let url = download_url().unwrap();
            assert!(url.contains("/25/ga/linux/x64/jdk/"));
        }
    }
}
