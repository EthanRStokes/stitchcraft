//! The Gradle build for the exported mod.
//!
//! The layout is the plainest multiloader arrangement that needs no third-party
//! plugin: `common/` is a source directory rather than a subproject, and each
//! loader adds it to its own source sets. That means the shared code is compiled
//! twice, once against Fabric's Minecraft and once against NeoForge's, which is
//! exactly what makes it work without something like Architectury standing in
//! the middle and without having to compile `common` against a stripped vanilla
//! jar.
//!
//! Plugin and dependency versions live in `gradle.properties` so they can be
//! bumped without touching a build script. The build scripts follow Fabric's
//! and NeoForge's own templates for the targeted game version (Minecraft 26.x
//! ships unobfuscated, so there are no mappings and no remapping).
//!
//! The export carries a Gradle wrapper, so `./gradlew build` works with
//! nothing but a JDK installed, and the editor's Run and Build buttons use the
//! same wrapper.
//!
//! Each loader's runs read one optional Gradle property, `stitchcraftDebug`.
//! The editor sets it to the address of its debugger when it starts a debug
//! run; the generated `Dbg` class connects there. Without it the runs are
//! ordinary dev runs and `Dbg` does nothing.

use crate::naming::Names;
use std::collections::BTreeMap;
use stitchcraft_blocks::ModProject;

/// The Gradle version the wrapper downloads.
pub const GRADLE_VERSION: &str = "9.8.2";

/// Every build file. Only the loaders the project asked for get a subproject.
pub fn files(project: &ModProject, names: &Names) -> Vec<(String, String)> {
    let mut out = vec![
        ("settings.gradle.kts".to_string(), settings(project)),
        ("build.gradle.kts".to_string(), root_build(project)),
        (
            "gradle.properties".to_string(),
            properties(project, names),
        ),
        (".gitignore".to_string(), gitignore()),
        ("gradlew".to_string(), include_str!("../../../res/gradle-wrapper/gradlew").to_string()),
        (
            "gradlew.bat".to_string(),
            include_str!("../../../res/gradle-wrapper/gradlew.bat").to_string(),
        ),
        (
            "gradle/wrapper/gradle-wrapper.properties".to_string(),
            include_str!("../../../res/gradle-wrapper/gradle-wrapper.properties").to_string(),
        ),
        // Loom needs Gradle itself to run on Java 25. This lets the wrapper
        // start on any Java 17 or newer and fetch a 25 for its daemon.
        (
            "gradle/gradle-daemon-jvm.properties".to_string(),
            include_str!("../../../res/gradle-wrapper/gradle-daemon-jvm.properties").to_string(),
        ),
    ];
    if project.loaders.fabric {
        out.push(("fabric/build.gradle.kts".to_string(), fabric_build()));
    }
    if project.loaders.neoforge {
        out.push(("neoforge/build.gradle.kts".to_string(), neoforge_build()));
    }
    out
}

/// The wrapper's jar, the one binary file in an export.
pub fn binaries() -> BTreeMap<String, &'static [u8]> {
    BTreeMap::from([(
        "gradle/wrapper/gradle-wrapper.jar".to_string(),
        &include_bytes!("../../../res/gradle-wrapper/gradle-wrapper.jar")[..],
    )])
}

/// Files written with the executable bit on Unix.
pub fn is_executable(path: &str) -> bool {
    path == "gradlew"
}

fn settings(project: &ModProject) -> String {
    let mut out = String::from(
        r#"pluginManagement {
    repositories {
        maven("https://maven.fabricmc.net/") { name = "Fabric" }
        maven("https://maven.neoforged.net/releases") { name = "NeoForged" }
        gradlePluginPortal()
        mavenCentral()
    }
    // Plugin versions come from gradle.properties, so the loader builds can
    // apply them by id alone.
    val loomVersion: String by settings
    val modDevVersion: String by settings
    plugins {
        id("net.fabricmc.fabric-loom") version loomVersion
        id("net.neoforged.moddev") version modDevVersion
    }
}

plugins {
    // Downloads the JDK the build asks for when it is not installed.
    id("org.gradle.toolchains.foojay-resolver-convention") version "1.0.0"
}

"#,
    );
    out.push_str(&format!("rootProject.name = \"{}\"\n\n", project.mod_id));
    out.push_str(
        "// `common/` is a source directory shared by both loaders rather than a\n\
         // subproject of its own - see each loader's build script.\n",
    );
    if project.loaders.fabric {
        out.push_str("include(\"fabric\")\n");
    }
    if project.loaders.neoforge {
        out.push_str("include(\"neoforge\")\n");
    }
    out
}

fn root_build(project: &ModProject) -> String {
    r#"// Settings every subproject shares. The loader-specific plugins are
// applied in the subprojects themselves, because Loom and ModDevGradle
// cannot both be on one project.
val modVersion: String by project
val javaVersion: String by project

subprojects {
    apply(plugin = "java")

    group = "@GROUP@"
    version = modVersion

    repositories {
        // FrozenLib is published to Modrinth's maven, which is the coordinate
        // the loader builds use.
        exclusiveContent {
            forRepository { maven("https://api.modrinth.com/maven") { name = "Modrinth" } }
            filter { includeGroup("maven.modrinth") }
        }
        // Fabric's own artifacts, asked for here first rather than after
        // Maven Central has been asked and said no.
        maven("https://maven.fabricmc.net/") {
            name = "Fabric"
            content { includeGroupByRegex("net\\.fabricmc.*") }
        }
        mavenCentral()
    }

    extensions.configure<JavaPluginExtension> {
        toolchain { languageVersion.set(JavaLanguageVersion.of(javaVersion.toInt())) }
        withSourcesJar()
    }

    tasks.withType<JavaCompile>().configureEach {
        options.encoding = "UTF-8"
        options.release.set(javaVersion.toInt())
    }
}
"#
    .replace("@GROUP@", &project.package)
}

fn properties(project: &ModProject, names: &Names) -> String {
    let targets = &project.targets;
    format!(
        "org.gradle.jvmargs=-Xmx3G\n\
         org.gradle.parallel=true\n\n\
         # Identity\n\
         modId={}\n\
         modName={}\n\
         modVersion={}\n\
         modPackage={}\n\
         javaVersion={}\n\n\
         # Game and loaders\n\
         minecraftVersion={}\n\
         fabricLoaderVersion={}\n\
         fabricApiVersion={}\n\
         neoforgeVersion={}\n\n\
         # FrozenLib, which the generated registry and event code is built on.\n\
         # The Modrinth file version without its loader; each build adds\n\
         # -fabric or -neoforge.\n\
         frozenlibVersion={}\n\n\
         # Build plugins. These track the game version rather than the mod, and\n\
         # are the first thing to check if the build fails before compiling.\n\
         loomVersion=1.19-SNAPSHOT\n\
         modDevVersion=2.0.148\n",
        project.mod_id,
        project.name,
        project.version,
        names.package,
        targets.java_version,
        targets.minecraft_version,
        targets.fabric_loader_version,
        targets.fabric_api_version,
        targets.neoforge_version,
        targets.frozenlib_version,
    )
}

fn fabric_build() -> String {
    String::from(
        r#"// The Fabric build. `common/` is pulled in as extra source rather than a
// project dependency, so it compiles against Fabric's Minecraft directly.
plugins {
    id("net.fabricmc.fabric-loom")
}

val minecraftVersion: String by project
val fabricLoaderVersion: String by project
val fabricApiVersion: String by project
val frozenlibVersion: String by project
val modId: String by project

base { archivesName.set("$modId-fabric") }

sourceSets {
    main {
        java.srcDir(rootProject.file("common/src/main/java"))
        resources.srcDir(rootProject.file("common/src/main/resources"))
    }
}

// Set by Stitchcraft's Debug button: where the editor's debugger listens.
val stitchcraftDebug: String? = providers.gradleProperty("stitchcraftDebug").orNull

loom {
    runs {
        configureEach {
            if (stitchcraftDebug != null) property("stitchcraft.debug", stitchcraftDebug)
        }
    }
}

dependencies {
    minecraft("com.mojang:minecraft:$minecraftVersion")
    implementation("net.fabricmc:fabric-loader:$fabricLoaderVersion")
    implementation("net.fabricmc.fabric-api:fabric-api:$fabricApiVersion")

    // FrozenLib supplies the deferred registries and the cross-platform
    // events the generated code is written against.
    implementation("maven.modrinth:frozenlib:$frozenlibVersion-fabric")
}
"#,
    )
}

fn neoforge_build() -> String {
    String::from(
        r#"// The NeoForge build, using NeoForge's own ModDevGradle. As on the Fabric
// side, `common/` is extra source rather than a project dependency.
plugins {
    id("net.neoforged.moddev")
}

val neoforgeVersion: String by project
val frozenlibVersion: String by project
val modId: String by project

base { archivesName.set("$modId-neoforge") }

// Set by Stitchcraft's Debug button: where the editor's debugger listens.
val stitchcraftDebug: String? = providers.gradleProperty("stitchcraftDebug").orNull

neoForge {
    version = neoforgeVersion

    runs {
        create("client") { client() }
        create("server") {
            server()
            programArgument("--nogui")
        }
        configureEach {
            if (stitchcraftDebug != null) systemProperty("stitchcraft.debug", stitchcraftDebug)
        }
    }

    mods {
        create(modId) { sourceSet(sourceSets.main.get()) }
    }
}

sourceSets {
    main {
        java.srcDir(rootProject.file("common/src/main/java"))
        resources.srcDir(rootProject.file("common/src/main/resources"))
    }
}

dependencies {
    implementation("maven.modrinth:frozenlib:$frozenlibVersion-neoforge")
}
"#,
    )
}

fn gitignore() -> String {
    String::from(
        "# Gradle\n\
         .gradle/\n\
         build/\n\
         */build/\n\n\
         # Development runs\n\
         run/\n\
         */run/\n\n\
         # IDEs\n\
         .idea/\n\
         *.iml\n\
         .vscode/\n\
         .eclipse/\n\
         bin/\n",
    )
}

/// The `README.md` the export drops next to the build, saying what has to be
/// confirmed or filled in before the mod will build and run.
pub fn readme(project: &ModProject, names: &Names, missing_art: &[String]) -> String {
    let mut out = format!(
        "# {}\n\n\
         Generated by Stitchcraft from a block canvas. Everything under \
         `common/`, `fabric/` and `neoforge/` is written fresh on each export, so \
         edit the canvas rather than the code unless you are ready to stop \
         exporting over it.\n\n\
         ## Layout\n\n\
         | Path | What's in it |\n\
         | --- | --- |\n\
         | `common/src/main/java/{pkg}/registry/` | The `DeferredRegister` classes - blocks, items, entities, sounds, creative tabs |\n\
         | `common/src/main/java/{pkg}/content/` | The block, item and mob classes those registries register |\n\
         | `common/src/main/java/{pkg}/script/` | The canvas itself: `Handlers` (one method per stack), `Hooks` (event wiring), `Rt`, `Vars`, `Scheduler` |\n\
         | `fabric/`, `neoforge/` | Entrypoints, metadata, and the per-loader command bridge |\n\n\
         `common/` is a shared source directory rather than a Gradle subproject: \
         each loader adds it to its own source sets, so it is compiled twice, \
         once against each loader's Minecraft. That is what lets the build work \
         without Architectury or a vanilla-only Minecraft jar.\n\n\
         `Rt.java` is the only generated file that calls Minecraft's own API. If \
         a game update renames something, that is the file to fix - not the \
         hundreds of call sites the canvas produced.\n\n\
         ## Before the first build\n\n\
         Stitchcraft pins what it can and guesses what it cannot. Check these in \
         `gradle.properties`:\n\n\
         - `loomVersion` and `modDevVersion` - build plugins, which track the \
         game version rather than the mod. A build that fails before compiling \
         anything is almost always one of these.\n\
         - `frozenlibVersion` - the generated code depends on FrozenLib through \
         Modrinth's maven (`maven.modrinth:frozenlib`). The version there is the \
         *file* version and differs per loader; the FrozenBlock maven is also \
         configured if you would rather use `net.frozenblock` coordinates.\n\
         - `minecraftVersion`, `fabricApiVersion`, `neoforgeVersion` - set from \
         the canvas's build targets ({mc}).\n\n\
         Build with `./gradlew build` (`gradlew.bat build` on Windows); the \
         jars land in `fabric/build/libs/` and `neoforge/build/libs/`. \
         `./gradlew :fabric:runClient` or `:neoforge:runServer` start a \
         development game.\n\n",
        project.name,
        pkg = names.package_dir(),
        mc = project.targets.minecraft_version,
    );

    if !missing_art.is_empty() {
        out.push_str(
            "## Art and audio\n\n\
             A canvas describes behaviour, not artwork, so the models and sound \
             index point at files that are not there yet. Add these and the \
             declarations will look and sound like themselves:\n\n",
        );
        for path in missing_art {
            out.push_str(&format!("- `{path}`\n"));
        }
        out.push('\n');
    }

    if !crate::content::declared_bases(project).is_empty() {
        out.push_str(
            "Declared entities are registered with a `NoopRenderer`, which keeps \
             the client from crashing but leaves them invisible. Replace those in \
             `content/ModRenderers.java` with a real `EntityRenderer` when you \
             want to see them.\n\n",
        );
    }

    out.push_str(&format!(
        "## Saved state\n\n\
         The canvas's variables, lists and dicts are declared at mod init and \
         saved to `<game dir>/stitchcraft/{}.json` when the server stops. That is \
         server-wide rather than per world, because a canvas has no notion of \
         which save it belongs to.\n",
        project.mod_id
    ));
    out
}
