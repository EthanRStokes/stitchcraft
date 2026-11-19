//! The `Backend` QObject: the one thing QML talks to.
//!
//! The canvas, the palette drags and the Make a Block editor all come from
//! blockstitch, and every edit they make arrives here as one
//! `invoke(command, args)` call in blockstitch's command vocabulary. The
//! command runs in [`stitchcraft_blocks::editing::Session`], which owns the
//! rules and the undo history and is tested without Qt; this file only
//! carries JSON across and pushes the document back out when it changed.
//!
//! The document crosses in blockstitch's frontend shape
//! (`graph::to_frontend`), so the canvas reads each instruction flat.

use blockstitch_core::graph;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use serde_json::{Value as Json, json};
use std::pin::Pin;
use stitchcraft_blocks::editing::Session;
use stitchcraft_blocks::{ModProject, catalog, reporters, validate};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        /// The editor's whole state, as QML sees it.
        ///
        /// `catalogJson` never changes: the block vocabulary, how it draws and
        /// where the palette files it. The rest follows the document.
        #[qobject]
        #[qml_element]
        #[qproperty(QString, document_json, cxx_name = "documentJson")]
        #[qproperty(QString, catalog_json, cxx_name = "catalogJson")]
        #[qproperty(QString, choices_json, cxx_name = "choicesJson")]
        #[qproperty(QString, diagnostics_json, cxx_name = "diagnosticsJson")]
        #[qproperty(QString, mod_id, cxx_name = "modId")]
        #[qproperty(QString, mod_name, cxx_name = "modName")]
        #[qproperty(QString, mod_version, cxx_name = "modVersion")]
        #[qproperty(QString, mod_package, cxx_name = "modPackage")]
        #[qproperty(bool, fabric)]
        #[qproperty(bool, neoforge)]
        #[qproperty(QString, document_path, cxx_name = "documentPath")]
        #[qproperty(bool, dirty)]
        #[qproperty(bool, can_undo, cxx_name = "canUndo")]
        #[qproperty(bool, can_redo, cxx_name = "canRedo")]
        #[qproperty(QString, status, cxx_name = "status")]
        type Backend = super::BackendRust;

        /// Runs one editor command. Answers `{"ok": true, "result": ...}` or
        /// `{"ok": false, "error": "..."}`.
        #[qinvokable]
        fn invoke(self: Pin<&mut Backend>, command: &QString, args: &QString) -> QString;

        #[qinvokable]
        #[cxx_name = "newProject"]
        fn new_project(self: Pin<&mut Backend>);

        #[qinvokable]
        fn open(self: Pin<&mut Backend>, path: &QString) -> bool;

        #[qinvokable]
        fn save(self: Pin<&mut Backend>, path: &QString) -> bool;

        #[qinvokable]
        #[cxx_name = "exportMod"]
        fn export_mod(self: Pin<&mut Backend>, directory: &QString) -> bool;

        #[qinvokable]
        #[cxx_name = "setMetadata"]
        fn set_metadata(
            self: Pin<&mut Backend>,
            mod_id: &QString,
            name: &QString,
            version: &QString,
            package: &QString,
            fabric: bool,
            neoforge: bool,
        );
    }
}

/// The editor's state. Only the fields before `session` are QML properties.
pub struct BackendRust {
    document_json: QString,
    catalog_json: QString,
    choices_json: QString,
    diagnostics_json: QString,
    mod_id: QString,
    mod_name: QString,
    mod_version: QString,
    mod_package: QString,
    fabric: bool,
    neoforge: bool,
    document_path: QString,
    dirty: bool,
    can_undo: bool,
    can_redo: bool,
    status: QString,

    session: Session,
    /// The session revision last pushed to QML.
    published: u64,
}

impl Default for BackendRust {
    fn default() -> Self {
        // Before any document is read, so saved reporters know their arity.
        reporters::register();

        // `stitchcraft <project.stitch>` opens that project. One that won't
        // load leaves a blank project and says why, rather than refusing to start.
        let (project, path, status) = match std::env::args().nth(1) {
            Some(path) => match load(&path) {
                Ok(project) => {
                    let message = format!("Opened {path}.");
                    (project, path, message)
                }
                Err(error) => (ModProject::default(), String::new(), error),
            },
            None => (ModProject::default(), String::new(), String::new()),
        };

        let mut backend = Self {
            document_json: QString::default(),
            catalog_json: QString::from(&catalog_json().to_string()),
            choices_json: QString::default(),
            diagnostics_json: QString::default(),
            mod_id: QString::default(),
            mod_name: QString::default(),
            mod_version: QString::default(),
            mod_package: QString::default(),
            fabric: true,
            neoforge: true,
            document_path: QString::from(&path),
            dirty: false,
            can_undo: false,
            can_redo: false,
            status: QString::from(&status),
            session: Session::new(project),
            published: 0,
        };
        // Derived here as well as in `refresh`, so an opened project is on the
        // canvas before the first edit.
        let derived = Derived::of(&backend.session);
        backend.document_json = QString::from(&derived.document);
        backend.choices_json = QString::from(&derived.choices);
        backend.diagnostics_json = QString::from(&derived.diagnostics);
        backend.mod_id = QString::from(&derived.mod_id);
        backend.mod_name = QString::from(&derived.name);
        backend.mod_version = QString::from(&derived.version);
        backend.mod_package = QString::from(&derived.package);
        backend.fabric = derived.fabric;
        backend.neoforge = derived.neoforge;
        backend
    }
}

/// Everything QML reads off the block vocabulary, in one object.
fn catalog_json() -> Json {
    json!({
        "rows": catalog::rows_json(),
        "prefabs": catalog::prefabs_json(),
        "operators": reporters::operator_rows_json(),
        "descriptors": reporters::descriptors_json(),
        "extraOperators": reporters::palette_entries_json(),
        "sections": catalog::sections_json(),
        "categories": catalog::categories_json(),
        "listTypes": catalog::LIST_TYPES,
        "dictTypes": catalog::DICT_TYPES,
    })
}

/// Reads a saved project, with the one-time repairs a loaded document needs.
fn load(path: &str) -> Result<ModProject, String> {
    let contents =
        std::fs::read_to_string(path).map_err(|error| format!("Couldn't open {path}: {error}"))?;
    let mut project: ModProject = serde_json::from_str(&contents)
        .map_err(|error| format!("{path} isn't a Stitchcraft project: {error}"))?;
    project.migrate_after_load();
    Ok(project)
}

fn text(value: &QString) -> String {
    value.to_string()
}

/// What the document's properties read, computed once per change.
struct Derived {
    document: String,
    choices: String,
    diagnostics: String,
    mod_id: String,
    name: String,
    version: String,
    package: String,
    fabric: bool,
    neoforge: bool,
}

impl Derived {
    fn of(session: &Session) -> Self {
        let project = &session.project;
        Self {
            document: graph::to_frontend(project)
                .map(|value| value.to_string())
                .unwrap_or_else(|_| "{}".to_string()),
            choices: choices_of(project),
            diagnostics: diagnostics_of(project),
            mod_id: project.mod_id.clone(),
            name: project.name.clone(),
            version: project.version.clone(),
            package: project.package.clone(),
            fabric: project.loaders.fabric,
            neoforge: project.loaders.neoforge,
        }
    }
}

impl qobject::Backend {
    /// Pushes the document out if it changed since the last push.
    fn refresh(mut self: Pin<&mut Self>) {
        let (revision, can_undo, can_redo) = {
            let session = &self.rust().session;
            (session.revision(), session.can_undo(), session.can_redo())
        };
        self.as_mut().set_can_undo(can_undo);
        self.as_mut().set_can_redo(can_redo);
        if revision == self.rust().published {
            return;
        }
        self.as_mut().rust_mut().published = revision;
        let d = Derived::of(&self.rust().session);
        self.as_mut().set_mod_id(QString::from(&d.mod_id));
        self.as_mut().set_mod_name(QString::from(&d.name));
        self.as_mut().set_mod_version(QString::from(&d.version));
        self.as_mut().set_mod_package(QString::from(&d.package));
        self.as_mut().set_fabric(d.fabric);
        self.as_mut().set_neoforge(d.neoforge);
        self.as_mut().set_choices_json(QString::from(&d.choices));
        self.as_mut().set_diagnostics_json(QString::from(&d.diagnostics));
        self.as_mut().set_document_json(QString::from(&d.document));
    }

    fn say(mut self: Pin<&mut Self>, message: impl AsRef<str>) {
        self.as_mut().set_status(QString::from(message.as_ref()));
    }

    fn invoke(mut self: Pin<&mut Self>, command: &QString, args: &QString) -> QString {
        let command = text(command);
        let args: Json = match serde_json::from_str(&text(args)) {
            Ok(args) => args,
            Err(error) => {
                let message = format!("The editor sent `{command}` arguments that aren't JSON: {error}");
                self.as_mut().say(&message);
                return QString::from(&json!({ "ok": false, "error": message }).to_string());
            }
        };
        let before = self.rust().session.revision();
        let result = self.as_mut().rust_mut().session.dispatch(&command, &args);
        let changed = self.rust().session.revision() != before;
        let answer = match result {
            Ok(value) => {
                if changed {
                    self.as_mut().set_dirty(true);
                    match command.as_str() {
                        "undo" => self.as_mut().say("Undone."),
                        "redo" => self.as_mut().say("Redone."),
                        _ => self.as_mut().say(""),
                    }
                }
                json!({ "ok": true, "result": value })
            }
            Err(error) => {
                self.as_mut().say(&error);
                json!({ "ok": false, "error": error })
            }
        };
        self.refresh();
        QString::from(&answer.to_string())
    }

    fn new_project(mut self: Pin<&mut Self>) {
        self.as_mut()
            .rust_mut()
            .session
            .replace(ModProject::default());
        self.as_mut().set_document_path(QString::from(""));
        self.as_mut().set_dirty(false);
        self.as_mut().say("New project.");
        self.refresh();
    }

    fn open(mut self: Pin<&mut Self>, path: &QString) -> bool {
        let path = text(path);
        match load(&path) {
            Ok(project) => {
                self.as_mut().rust_mut().session.replace(project);
                self.as_mut().set_document_path(QString::from(&path));
                self.as_mut().set_dirty(false);
                self.as_mut().say(format!("Opened {path}."));
                self.refresh();
                true
            }
            Err(error) => {
                self.say(error);
                false
            }
        }
    }

    fn save(mut self: Pin<&mut Self>, path: &QString) -> bool {
        let mut path = text(path);
        if path.is_empty() {
            path = text(&self.rust().document_path);
        }
        if path.is_empty() {
            self.say("This project has no file yet. Choose where to save it.");
            return false;
        }
        let contents = match serde_json::to_string_pretty(&self.rust().session.project) {
            Ok(contents) => contents,
            Err(error) => {
                self.say(format!("Couldn't write the project: {error}"));
                return false;
            }
        };
        if let Err(error) = std::fs::write(&path, contents) {
            self.say(format!("Couldn't write {path}: {error}"));
            return false;
        }
        self.as_mut().set_document_path(QString::from(&path));
        self.as_mut().set_dirty(false);
        self.say(format!("Saved to {path}."));
        true
    }

    fn export_mod(self: Pin<&mut Self>, directory: &QString) -> bool {
        let directory = text(directory);
        if directory.is_empty() {
            self.say("Choose a folder to export into.");
            return false;
        }
        let bundle = match stitchcraft_export::export(&self.rust().session.project) {
            Ok(bundle) => bundle,
            Err(report) => {
                let first = report
                    .errors()
                    .next()
                    .map(|diagnostic| diagnostic.message.clone())
                    .unwrap_or_else(|| "the canvas has errors".to_string());
                let count = report.errors().count();
                self.say(if count > 1 {
                    format!("{first} (and {} more)", count - 1)
                } else {
                    first
                });
                return false;
            }
        };
        if let Err(error) = bundle.write_to(&directory) {
            self.say(format!("Couldn't write to {directory}: {error}"));
            return false;
        }
        let mut message = format!("Exported {} files to {directory}.", bundle.files.len());
        if let Some(warning) = bundle.warnings.first() {
            message.push(' ');
            message.push_str(warning);
        }
        self.say(message);
        true
    }

    fn set_metadata(
        mut self: Pin<&mut Self>,
        mod_id: &QString,
        name: &QString,
        version: &QString,
        package: &QString,
        fabric: bool,
        neoforge: bool,
    ) {
        {
            let mut rust = self.as_mut().rust_mut();
            let project = &mut rust.session.project;
            project.mod_id = stitchcraft_blocks::document::sanitize_mod_id(&text(mod_id));
            project.name = text(name);
            project.version = text(version);
            project.package = text(package);
            project.loaders.fabric = fabric;
            project.loaders.neoforge = neoforge;
            rust.session.changed();
        }
        self.as_mut().set_dirty(true);
        self.refresh();
    }
}

/// The dynamic dropdown choices the rows refer to by name - see
/// [`catalog::DYNAMIC_SOURCES`].
fn choices_of(project: &ModProject) -> String {
    let declarations = validate::Declarations::gather(project);
    json!({
        "declaredBlocks": declarations.blocks,
        "declaredItems": declarations.items,
        "declaredEntities": declarations.entities,
        "declaredSounds": declarations.sounds,
        "declaredTabs": declarations.tabs,
    })
    .to_string()
}

/// The document's diagnostics, as `[{severity, message, strandId}]`.
fn diagnostics_of(project: &ModProject) -> String {
    let report = validate::check(project);
    json!(
        report
            .diagnostics
            .iter()
            .map(|diagnostic| json!({
                "severity": match diagnostic.severity {
                    validate::Severity::Error => "error",
                    validate::Severity::Warning => "warning",
                },
                "message": diagnostic.message,
                "strandId": diagnostic.strand_id,
            }))
            .collect::<Vec<_>>()
    )
    .to_string()
}
