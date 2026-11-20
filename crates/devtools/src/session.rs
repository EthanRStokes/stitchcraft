//! [`DevTools`]: one object the app holds, asks to do things, and polls for
//! what happened.
//!
//! Everything here returns at once. Gradle, the games it starts, a Java
//! download and the debugger all run on their own threads or processes, and
//! [`DevTools::poll`] hands back what they did since the last call as
//! [`Event`]s, which serialize to the JSON the app's QML reads.

use crate::debugger::{Debugger, Message};
use crate::java::{self, Java};
use crate::process::Job;
use crate::workspace;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{Receiver, channel};
use stitchcraft_blocks::ModProject;

/// A mod loader the export targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Loader {
    Fabric,
    NeoForge,
}

impl Loader {
    /// The Gradle subproject, and the name the app passes in.
    pub fn id(self) -> &'static str {
        match self {
            Loader::Fabric => "fabric",
            Loader::NeoForge => "neoforge",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Loader::Fabric => "Fabric",
            Loader::NeoForge => "NeoForge",
        }
    }

    pub fn parse(text: &str) -> Option<Loader> {
        match text {
            "fabric" => Some(Loader::Fabric),
            "neoforge" => Some(Loader::NeoForge),
            _ => None,
        }
    }
}

/// What a job does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    /// Compile the mod's jars.
    Build,
    /// Start a development client with the mod in it.
    Client,
    /// Start a development dedicated server with the mod in it.
    Server,
}

impl Action {
    pub fn parse(text: &str) -> Option<Action> {
        match text {
            "build" => Some(Action::Build),
            "client" => Some(Action::Client),
            "server" => Some(Action::Server),
            _ => None,
        }
    }

    fn task(self) -> &'static str {
        match self {
            Action::Build => "build",
            Action::Client => "runClient",
            Action::Server => "runServer",
        }
    }
}

/// Why [`DevTools::start`] did not start anything.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "reason", rename_all = "camelCase")]
pub enum Refusal {
    /// No Java 17+ was found. [`DevTools::download_java`] fixes it.
    NeedJava,
    /// A server was asked for before the Minecraft EULA was accepted.
    /// [`DevTools::accept_eula`] fixes it.
    NeedEula { loader: Loader },
    /// That job is already running.
    AlreadyRunning { job: u32 },
    /// Anything else, said for the user.
    Failed { message: String },
}

/// Something that happened, for the app.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Event {
    /// A job started.
    Started {
        job: u32,
        action: Action,
        loaders: Vec<Loader>,
        debug: bool,
        title: String,
    },
    /// A job printed a line.
    Line { job: u32, text: String },
    /// A job ended. `jars` are what a successful build produced.
    Finished {
        job: u32,
        ok: bool,
        stopped: bool,
        code: Option<i32>,
        jars: Vec<String>,
    },
    /// Java download progress, in bytes. `total` is 0 when unknown.
    JavaProgress { done: u64, total: u64 },
    JavaReady { version: u32, home: String },
    JavaFailed { message: String },
    /// A debugged game connected.
    Connected { side: String },
    /// A debugged game went away.
    Disconnected { side: String },
    /// Block `id` just ran.
    At { side: String, id: String },
    /// The game is stopped before block `id`.
    Paused {
        side: String,
        id: String,
        vars: serde_json::Value,
    },
    /// The canvas's variables, as of now.
    Vars {
        side: String,
        vars: serde_json::Value,
    },
    /// A block failed. `id` is `-` when no block was running.
    Failed {
        side: String,
        id: String,
        message: String,
    },
}

struct Running {
    id: u32,
    action: Action,
    loaders: Vec<Loader>,
    workspace: PathBuf,
    job: Job,
}

/// The app's dev tools. See the module docs.
pub struct DevTools {
    data_dir: PathBuf,
    java: Option<Java>,
    downloading: Option<Receiver<Event>>,
    jobs: Vec<Running>,
    next_job: u32,
    debugger: Option<Debugger>,
    /// Started events not yet polled.
    pending_started: Vec<Event>,
    /// Extra Gradle arguments, for the environment rather than the user
    /// (`STITCHCRAFT_GRADLE_ARGS`, split on spaces).
    extra_args: Vec<String>,
}

impl DevTools {
    pub fn new(data_dir: PathBuf) -> DevTools {
        let java = java::find(&data_dir);
        let extra_args = std::env::var("STITCHCRAFT_GRADLE_ARGS")
            .map(|args| args.split_whitespace().map(str::to_string).collect())
            .unwrap_or_default();
        DevTools {
            data_dir,
            java,
            downloading: None,
            jobs: Vec::new(),
            next_job: 1,
            debugger: None,
            pending_started: Vec::new(),
            extra_args,
        }
    }

    pub fn java(&self) -> Option<&Java> {
        self.java.as_ref()
    }

    pub fn downloading_java(&self) -> bool {
        self.downloading.is_some()
    }

    /// Fetches Temurin in the background; progress arrives through [`poll`].
    ///
    /// [`poll`]: DevTools::poll
    pub fn download_java(&mut self) {
        if self.downloading.is_some() {
            return;
        }
        let (sender, receiver) = channel();
        let data_dir = self.data_dir.clone();
        std::thread::spawn(move || {
            let mut last = 0u64;
            let progress = sender.clone();
            let result = java::download(&data_dir, |done, total| {
                // A message per megabyte is plenty for a progress bar.
                if done - last >= 1 << 20 || done == total {
                    last = done;
                    let _ = progress.send(Event::JavaProgress { done, total });
                }
            });
            let _ = sender.send(match result {
                Ok(java) => Event::JavaReady {
                    version: java.version,
                    home: java.home.display().to_string(),
                },
                Err(message) => Event::JavaFailed { message },
            });
        });
        self.downloading = Some(receiver);
    }

    /// The private Gradle project for `mod_id`.
    pub fn workspace(&self, mod_id: &str) -> PathBuf {
        workspace::dir(&self.data_dir, mod_id)
    }

    pub fn eula_accepted(&self, mod_id: &str, loader: Loader) -> bool {
        workspace::eula_accepted(&self.workspace(mod_id), loader.id())
    }

    pub fn accept_eula(&self, mod_id: &str, loader: Loader) -> Result<(), String> {
        workspace::accept_eula(&self.workspace(mod_id), loader.id()).map_err(|e| e.to_string())
    }

    /// Starts `action`. A build covers every loader in `loaders`; a client or
    /// server run uses the first. `debug` instruments the canvas and
    /// connects the game to the debugger.
    pub fn start(
        &mut self,
        project: &ModProject,
        action: Action,
        loaders: &[Loader],
        debug: bool,
    ) -> Result<u32, Refusal> {
        let Some(java) = self.java.clone() else {
            return Err(Refusal::NeedJava);
        };
        let loaders: Vec<Loader> = match action {
            Action::Build => loaders.to_vec(),
            _ => loaders.iter().take(1).copied().collect(),
        };
        if loaders.is_empty() {
            return Err(Refusal::Failed {
                message: "Turn on Fabric or NeoForge in Mod settings first.".into(),
            });
        }
        if let Some(busy) = self.jobs.iter().find(|job| {
            job.action == action && (action == Action::Build || job.loaders == loaders)
        }) {
            return Err(Refusal::AlreadyRunning { job: busy.id });
        }
        // Two jobs exporting into one workspace at once would trip over each
        // other's files, and a build would package a debug export.
        if let Some(other) = self.jobs.first() {
            let same_flavour = self.debugger.is_some() == debug;
            if !same_flavour || action == Action::Build || other.action == Action::Build {
                return Err(Refusal::Failed {
                    message: "Stop what is running first: a build and a run, or a debug run and a plain one, can't share the workspace.".into(),
                });
            }
        }
        if action == Action::Server && !self.eula_accepted(&project.mod_id, loaders[0]) {
            return Err(Refusal::NeedEula { loader: loaders[0] });
        }

        let workspace = if self.jobs.is_empty() {
            workspace::prepare(&self.data_dir, project, debug)
                .map_err(|message| Refusal::Failed { message })?
        } else {
            self.workspace(&project.mod_id)
        };

        if debug && self.debugger.is_none() {
            self.debugger = Some(Debugger::start().map_err(|e| Refusal::Failed {
                message: format!("Could not start the debugger: {e}"),
            })?);
        }
        if !debug {
            self.debugger = None;
        }

        let mut args = vec!["--console=plain".to_string()];
        args.extend(self.extra_args.iter().cloned());
        for loader in &loaders {
            args.push(format!(":{}:{}", loader.id(), action.task()));
        }
        if let Some(debugger) = &self.debugger {
            // The game can't always tell which side it is without loading a
            // client class, so the run says it.
            let side = if matches!(action, Action::Client) { "client" } else { "server" };
            args.push(format!("-PstitchcraftDebug={}/{side}", debugger.address()));
        }
        let mut command = Command::new(gradlew(&workspace));
        command
            .args(&args)
            .current_dir(&workspace)
            .env("JAVA_HOME", &java.home);
        let job = Job::spawn(command).map_err(|e| Refusal::Failed {
            message: format!("Could not start Gradle: {e}"),
        })?;

        let id = self.next_job;
        self.next_job += 1;
        let names: Vec<&str> = loaders.iter().map(|l| l.label()).collect();
        let title = match action {
            Action::Build => format!("Build {}", names.join(" + ")),
            Action::Client => format!("{} client", names[0]),
            Action::Server => format!("{} server", names[0]),
        };
        self.pending_started.push(Event::Started {
            job: id,
            action,
            loaders: loaders.clone(),
            debug,
            title,
        });
        self.jobs.push(Running {
            id,
            action,
            loaders,
            workspace,
            job,
        });
        Ok(id)
    }

    /// Stops a job; a second call forces it.
    pub fn stop(&mut self, job: u32) {
        if let Some(running) = self.jobs.iter_mut().find(|r| r.id == job) {
            running.job.stop();
        }
    }

    pub fn stop_all(&mut self) {
        for running in &mut self.jobs {
            running.job.stop();
        }
    }

    /// Types a line into a job, like a command into a server's console.
    pub fn send_input(&mut self, job: u32, line: &str) {
        if let Some(running) = self.jobs.iter_mut().find(|r| r.id == job) {
            let _ = running.job.send(line);
        }
    }

    pub fn running(&self) -> Vec<u32> {
        self.jobs.iter().map(|r| r.id).collect()
    }

    pub fn debugging(&self) -> bool {
        self.debugger.is_some()
    }

    pub fn set_breakpoints(&self, ids: Vec<String>) {
        if let Some(debugger) = &self.debugger {
            debugger.set_breakpoints(ids);
        }
    }

    /// `continue`, `step`, `pause` or `vars`, to one side or every game.
    pub fn debug_command(&self, side: Option<&str>, command: &str) {
        if let Some(debugger) = &self.debugger {
            debugger.send(side, command);
        }
    }

    /// Everything that happened since the last call.
    pub fn poll(&mut self) -> Vec<Event> {
        let mut events = std::mem::take(&mut self.pending_started);

        if let Some(receiver) = &self.downloading {
            for event in receiver.try_iter() {
                let done = matches!(event, Event::JavaReady { .. } | Event::JavaFailed { .. });
                events.push(event);
                if done {
                    self.java = java::find(&self.data_dir);
                    self.downloading = None;
                    break;
                }
            }
        }

        let mut finished = Vec::new();
        for (index, running) in self.jobs.iter_mut().enumerate() {
            for text in running.job.take_lines() {
                events.push(Event::Line {
                    job: running.id,
                    text,
                });
            }
            if let Some(status) = running.job.finished() {
                // The pump threads may still hold the last lines.
                std::thread::sleep(std::time::Duration::from_millis(30));
                for text in running.job.take_lines() {
                    events.push(Event::Line {
                        job: running.id,
                        text,
                    });
                }
                let ok = status.success();
                let jars = if ok && running.action == Action::Build {
                    running
                        .loaders
                        .iter()
                        .flat_map(|l| workspace::jars(&running.workspace, l.id()))
                        .map(|p| p.display().to_string())
                        .collect()
                } else {
                    Vec::new()
                };
                events.push(Event::Finished {
                    job: running.id,
                    ok,
                    stopped: running.job.stopping(),
                    code: status.code(),
                    jars,
                });
                finished.push(index);
            }
        }
        for index in finished.into_iter().rev() {
            self.jobs.remove(index);
        }

        if let Some(debugger) = &self.debugger {
            for message in debugger.take_messages() {
                events.push(match message {
                    Message::Hello { side } => Event::Connected { side },
                    Message::Gone { side } => Event::Disconnected { side },
                    Message::At { side, id } => Event::At { side, id },
                    Message::Paused { side, id, vars } => Event::Paused { side, id, vars },
                    Message::Vars { side, vars } => Event::Vars { side, vars },
                    Message::Error { side, id, message } => Event::Failed { side, id, message },
                });
            }
        }
        events
    }
}

/// The wrapper script in a workspace.
fn gradlew(workspace: &Path) -> PathBuf {
    workspace.join(if cfg!(windows) { "gradlew.bat" } else { "gradlew" })
}

/// Copies built jars into `directory`, answering where they went.
pub fn copy_jars(jars: &[String], directory: &Path) -> Result<Vec<PathBuf>, String> {
    std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    jars.iter()
        .map(|jar| {
            let source = Path::new(jar);
            let target = directory.join(source.file_name().ok_or("not a file")?);
            std::fs::copy(source, &target).map_err(|e| format!("Could not copy {jar}: {e}"))?;
            Ok(target)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_serialize_for_qml() {
        let event = Event::Paused {
            side: "server".into(),
            id: "i1".into(),
            vars: serde_json::json!({"variables": {"score": 1}}),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["kind"], "paused");
        assert_eq!(json["vars"]["variables"]["score"], 1);
        let refusal = serde_json::to_value(Refusal::NeedEula {
            loader: Loader::NeoForge,
        })
        .unwrap();
        assert_eq!(refusal, serde_json::json!({"reason": "needEula", "loader": "neoforge"}));
    }

    #[test]
    fn no_java_means_asking_for_it() {
        let data = std::env::temp_dir().join(format!("stitchcraft-nojava-{}", std::process::id()));
        let mut tools = DevTools::new(data);
        tools.java = None;
        let project = ModProject::new("x");
        assert_eq!(
            tools.start(&project, Action::Build, &[Loader::Fabric], false),
            Err(Refusal::NeedJava)
        );
    }
}
