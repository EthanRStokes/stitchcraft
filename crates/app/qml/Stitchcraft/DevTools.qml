import QtQuick

// The editor's side of the dev tools: building jars, development runs and
// the debugger. It polls the backend's `dev("poll")` for what Gradle, the
// games and the debugger did, and keeps the state the toolbar, the console
// and the canvas draw from.
//
// A refusal that the user can fix (no Java yet, the Minecraft EULA not yet
// accepted for servers) is raised as `javaNeeded` / `eulaNeeded`; the
// window answers it with a dialog and calls `retry()`.
QtObject {
    id: root

    required property var backend

    // ── Jobs ─────────────────────────────────────────────────────────────
    // [{ id, title, action, loaders, debug, running, ok, stopped, jars }].
    // Each job's console lines live in `consoles[id]`, a ListModel.
    property var jobs: []
    property var consoles: ({})
    property int currentJob: 0
    readonly property var current: jobs.find(j => j.id === currentJob) || null
    readonly property bool busy: jobs.some(j => j.running)
    readonly property bool building: jobs.some(j => j.running && j.action === "build")
    readonly property bool runningGame: jobs.some(j => j.running && j.action !== "build")
    readonly property int maxLines: 5000

    // ── Debugger ─────────────────────────────────────────────────────────
    property var sides: []                 // connected games: "client", "server"
    readonly property bool debugging: sides.length > 0
    property var breakpoints: []           // instruction ids
    property var paused: null              // { side, id, vars } while stopped
    property var vars: null                // the latest variables snapshot
    property var failures: ({})            // instruction id -> message
    property var recent: ({})              // instruction id -> time it last ran
    property var runningIds: []
    readonly property var pausedIds: paused ? [paused.id] : []

    // ── Java ─────────────────────────────────────────────────────────────
    property var java: ({ found: false, downloading: false })
    property real javaProgress: 0

    signal javaNeeded()
    signal eulaNeeded(string loader)
    signal jarsReady(var jars)
    signal failed(string message)

    // The last start request, for `retry()` after a refusal is dealt with.
    property var pending: null

    function call(command, args) {
        return JSON.parse(backend.dev(command, JSON.stringify(args || {})));
    }

    function refreshJava() { java = call("java"); }

    // action: "build" | "client" | "server"; loaders: ["fabric", ...].
    function start(action, loaders, debug) {
        pending = { action: action, loaders: loaders, debug: !!debug };
        const answer = call("start", pending);
        if (answer.ok) {
            currentJob = answer.job;
            pending = null;
            if (debug) call("breakpoints", { ids: breakpoints });
            poll();
            return true;
        }
        if (answer.refusal) {
            const reason = answer.refusal.reason;
            if (reason === "needJava") { javaNeeded(); return false; }
            if (reason === "needEula") { eulaNeeded(answer.refusal.loader); return false; }
            if (reason === "alreadyRunning") { currentJob = answer.refusal.job; pending = null; return false; }
        }
        pending = null;
        failed(answer.error || qsTr("That couldn't start."));
        return false;
    }
    function retry() { if (pending) start(pending.action, pending.loaders, pending.debug); }
    function acceptEula(loader) {
        const answer = call("acceptEula", { loader: loader });
        if (answer.ok) retry(); else failed(answer.error);
    }
    function downloadJava() { call("downloadJava"); refreshJava(); }

    function stop(job) { call("stop", { job: job }); }
    function stopAll() { call("stopAll"); }
    function sendInput(job, line) {
        const model = consoles[job];
        if (model) appendLine(model, "> " + line);
        call("input", { job: job, line: line });
    }
    function closeJob(job) {
        const entry = jobs.find(j => j.id === job);
        if (!entry || entry.running) return;
        const next = Object.assign({}, consoles);
        if (next[job]) next[job].destroy();
        delete next[job];
        consoles = next;
        jobs = jobs.filter(j => j.id !== job);
        if (currentJob === job) currentJob = jobs.length ? jobs[jobs.length - 1].id : 0;
    }
    function workspacePath() { return call("workspace").path; }

    // ── Breakpoints and stepping ─────────────────────────────────────────
    function toggleBreakpoint(id) {
        const i = breakpoints.indexOf(id);
        breakpoints = i >= 0 ? breakpoints.filter(b => b !== id) : breakpoints.concat([id]);
        call("breakpoints", { ids: breakpoints });
    }
    function clearBreakpoints() { breakpoints = []; call("breakpoints", { ids: [] }); }
    function debugCommand(command) {
        const side = paused ? paused.side : "";
        if (command !== "pause" && command !== "vars") paused = null;
        call("debug", { side: side, command: command });
    }
    function clearFailures() { failures = ({}); }

    // ── Polling ──────────────────────────────────────────────────────────
    readonly property Component consoleModel: Component { ListModel {} }

    function appendLine(model, text) {
        model.append({ line: text });
        if (model.count > maxLines) model.remove(0, model.count - maxLines);
    }

    function updateJob(id, change) {
        jobs = jobs.map(j => j.id === id ? Object.assign({}, j, change) : j);
    }

    function poll() {
        const answer = call("poll");
        if (!answer.ok) return;
        let ran = false;
        const now = Date.now();
        for (const e of answer.events) {
            switch (e.kind) {
            case "started": {
                const next = Object.assign({}, consoles);
                next[e.job] = consoleModel.createObject(root);
                consoles = next;
                jobs = jobs.concat([{ id: e.job, title: e.title, action: e.action, loaders: e.loaders,
                                      debug: e.debug, running: true, ok: false, stopped: false, jars: [] }]);
                if (e.debug) { failures = ({}); vars = null; }
                break;
            }
            case "line":
                if (consoles[e.job]) appendLine(consoles[e.job], e.text);
                break;
            case "finished":
                updateJob(e.job, { running: false, ok: e.ok, stopped: e.stopped, jars: e.jars });
                if (consoles[e.job])
                    appendLine(consoles[e.job], e.stopped ? qsTr("Stopped.")
                        : e.ok ? qsTr("Finished.") : qsTr("Ended with an error (exit code %1).").arg(e.code));
                if (e.ok && e.jars.length) jarsReady(e.jars);
                break;
            case "javaProgress":
                javaProgress = e.total > 0 ? e.done / e.total : 0;
                break;
            case "javaReady":
                refreshJava();
                retry();
                break;
            case "javaFailed":
                refreshJava();
                failed(e.message);
                break;
            case "connected":
                if (sides.indexOf(e.side) < 0) sides = sides.concat([e.side]);
                call("debug", { side: e.side, command: "vars" });
                break;
            case "disconnected":
                sides = sides.filter(s => s !== e.side);
                if (paused && paused.side === e.side) paused = null;
                break;
            case "at":
                recent[e.id] = now;
                ran = true;
                break;
            case "paused":
                paused = { side: e.side, id: e.id, vars: e.vars };
                vars = e.vars;
                break;
            case "vars":
                vars = e.vars;
                break;
            case "failed": {
                const next = Object.assign({}, failures);
                if (e.id !== "-") next[e.id] = e.message;
                failures = next;
                break;
            }
            }
        }
        // A block glows while it ran in the last moment, so a busy tick hook
        // stays lit and a one-off flashes.
        let changed = ran;
        for (const id in recent) if (now - recent[id] > 700) { delete recent[id]; changed = true; }
        if (changed) runningIds = Object.keys(recent);
    }

    property Timer pollTimer: Timer {
        interval: root.busy || root.java.downloading ? 100 : 1000
        running: true
        repeat: true
        onTriggered: root.poll()
    }
    // Live variables while a debugged game runs.
    property Timer varsTimer: Timer {
        interval: 1000
        running: root.debugging && !root.paused
        repeat: true
        onTriggered: root.call("debug", { side: "", command: "vars" })
    }

    Component.onCompleted: refreshJava()
}
