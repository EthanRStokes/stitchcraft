//! The private Gradle project each mod is built and run from.
//!
//! Every Run and Build exports the canvas into
//! `<data dir>/workspaces/<mod id>`, writing only the files whose contents
//! changed so Gradle's up-to-date checks keep working, and deleting generated
//! files the canvas no longer produces. That directory is Stitchcraft's own,
//! unlike an Export folder, so cleaning it is safe.

use std::path::{Path, PathBuf};
use stitchcraft_blocks::ModProject;
use stitchcraft_export::{Options, export_with};

/// The workspace for `mod_id`.
pub fn dir(data_dir: &Path, mod_id: &str) -> PathBuf {
    data_dir.join("workspaces").join(mod_id)
}

/// Exports `project` into its workspace. Answers the workspace, or the
/// first problem that stops the canvas exporting.
pub fn prepare(data_dir: &Path, project: &ModProject, debug: bool) -> Result<PathBuf, String> {
    let bundle = export_with(project, Options { debug }).map_err(|report| {
        let count = report.errors().count();
        let first = report
            .errors()
            .next()
            .map(|d| d.message.clone())
            .unwrap_or_default();
        if count > 1 {
            format!("{count} problems to fix first. The first: {first}")
        } else {
            format!("Fix this first: {first}")
        }
    })?;
    let root = dir(data_dir, &project.mod_id);
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;

    for path in bundle.stale_files(&root) {
        let _ = std::fs::remove_file(root.join(path));
    }
    // A loader switched off leaves its whole subproject behind.
    for (loader, on) in [("fabric", project.loaders.fabric), ("neoforge", project.loaders.neoforge)] {
        if !on {
            let _ = std::fs::remove_dir_all(root.join(loader).join("src"));
            let _ = std::fs::remove_file(root.join(loader).join("build.gradle.kts"));
        }
    }

    let unchanged = |path: &Path, contents: &[u8]| {
        std::fs::read(path).map(|old| old == contents).unwrap_or(false)
    };
    let mut changed = stitchcraft_export::Bundle {
        files: Default::default(),
        binaries: Default::default(),
        warnings: Vec::new(),
    };
    for (path, contents) in bundle.files {
        if !unchanged(&root.join(&path), contents.as_bytes()) {
            changed.files.insert(path, contents);
        }
    }
    for (path, contents) in bundle.binaries {
        if !unchanged(&root.join(&path), contents) {
            changed.binaries.insert(path, contents);
        }
    }
    changed.write_to(&root).map_err(|e| format!("Could not write the build: {e}"))?;
    Ok(root)
}

/// The run directory a loader's development game uses.
pub fn run_dir(workspace: &Path, loader: &str) -> PathBuf {
    workspace.join(loader).join("run")
}

/// True once the Minecraft EULA has been accepted for this workspace's
/// development servers.
pub fn eula_accepted(workspace: &Path, loader: &str) -> bool {
    std::fs::read_to_string(run_dir(workspace, loader).join("eula.txt"))
        .map(|text| text.lines().any(|line| line.trim() == "eula=true"))
        .unwrap_or(false)
}

/// Records the user's acceptance of the Minecraft EULA, which a server will
/// not start without, and gives a fresh development server settings that suit
/// testing: offline mode so the development client (which has no Microsoft
/// account) can join, and no watchdog, so a paused breakpoint does not get
/// the server killed for taking too long over one tick.
pub fn accept_eula(workspace: &Path, loader: &str) -> std::io::Result<()> {
    let run = run_dir(workspace, loader);
    std::fs::create_dir_all(&run)?;
    std::fs::write(
        run.join("eula.txt"),
        "# Accepted in Stitchcraft: https://aka.ms/MinecraftEULA\neula=true\n",
    )?;
    let properties = run.join("server.properties");
    if !properties.exists() {
        std::fs::write(
            properties,
            "online-mode=false\nmax-tick-time=-1\nspawn-protection=0\nmotd=Stitchcraft test server\n",
        )?;
    }
    Ok(())
}

/// The jars a build left for `loader`, without the sources jars.
pub fn jars(workspace: &Path, loader: &str) -> Vec<PathBuf> {
    let libs = workspace.join(loader).join("build").join("libs");
    let mut out: Vec<PathBuf> = std::fs::read_dir(libs)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    name.ends_with(".jar") && !name.ends_with("-sources.jar")
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use blockstitch_core::graph::Instruction;
    use stitchcraft_blocks::{McBlock, catalog};

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("stitchcraft-ws-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn project() -> ModProject {
        let mut project = ModProject::new("test_mod");
        project.graph.add_strand(
            0,
            0,
            vec![
                Instruction::new(McBlock::OnPlayerJoin),
                Instruction::new(catalog::prefab("Broadcast").unwrap()),
            ],
        );
        project
    }

    #[test]
    fn a_debug_build_carries_the_debugger_and_a_plain_one_drops_it() {
        let data = scratch("debug");
        let root = prepare(&data, &project(), true).unwrap();
        let dbg = root.join("common/src/main/java/com/example/test_mod/script/Dbg.java");
        assert!(dbg.exists(), "{dbg:?}");
        assert!(root.join("gradlew").exists());
        assert!(root.join("gradle/wrapper/gradle-wrapper.jar").exists());

        prepare(&data, &project(), false).unwrap();
        assert!(!dbg.exists(), "a stale debugger survived a plain build");
        let _ = std::fs::remove_dir_all(data);
    }

    #[test]
    fn unchanged_files_are_left_alone() {
        let data = scratch("mtime");
        let root = prepare(&data, &project(), false).unwrap();
        let file = root.join("settings.gradle.kts");
        let before = std::fs::metadata(&file).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        prepare(&data, &project(), false).unwrap();
        assert_eq!(std::fs::metadata(&file).unwrap().modified().unwrap(), before);
        let _ = std::fs::remove_dir_all(data);
    }

    #[test]
    fn accepting_the_eula_sets_up_a_test_server() {
        let data = scratch("eula");
        assert!(!eula_accepted(&data, "fabric"));
        accept_eula(&data, "fabric").unwrap();
        assert!(eula_accepted(&data, "fabric"));
        let properties = std::fs::read_to_string(run_dir(&data, "fabric").join("server.properties")).unwrap();
        assert!(properties.contains("online-mode=false"));
        let _ = std::fs::remove_dir_all(data);
    }

    #[test]
    fn a_canvas_with_errors_is_refused() {
        let data = scratch("invalid");
        let mut project = project();
        project.mod_id = "Not Valid".into();
        assert!(prepare(&data, &project, false).is_err());
        let _ = std::fs::remove_dir_all(data);
    }
}
