import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs
import QtQuick.Effects
import com.blockworked.Blockstitch
import com.blockworked.Stitchcraft

// The editor window: a toolbar, blockstitch's palette and canvas, and a
// status strip that says whether the canvas can be exported.
//
// Every edit is one `invoke(command, args)` into the Rust backend, in
// blockstitch's command vocabulary; the backend answers by replacing
// `documentJson`, and the canvas redraws from it.
ApplicationWindow {
    id: window

    // The frameless window, as Blockwork draws it. QWindowKit (see
    // window/WindowChrome.qml) moves, snaps and maximizes it; Linux also gets
    // client-side rounded corners and a shadow drawn inside transparent
    // margins, while Windows and macOS keep their native frame and shadow.
    readonly property bool customChrome: Qt.platform.os === "linux"
    readonly property real shadowPadding: customChrome ? 18 : 0
    readonly property real shadowMargin: visibility === Window.Windowed ? shadowPadding : 0
    readonly property real cornerRadius: customChrome && visibility === Window.Windowed ? 10 : 0
    readonly property var windowChrome: chromeLoader.item
    Loader {
        id: chromeLoader
        source: "qrc:/window/WindowChrome.qml"
        onLoaded: {
            item.dark = Qt.binding(() => Theme.dark);
            item.attach(window);
        }
        onStatusChanged: if (status === Loader.Error) {
            console.error("Could not load QWindowKit window integration");
            Qt.quit();
        }
    }

    width: 1440 + 2 * shadowPadding
    height: 900 + 2 * shadowPadding
    minimumWidth: 900 + 2 * shadowPadding
    minimumHeight: 560 + 2 * shadowPadding
    visible: windowChrome !== null && windowChrome.ready
    flags: Qt.Window | Qt.WindowMinMaxButtonsHint | Qt.WindowCloseButtonHint | Qt.WindowSystemMenuHint
    color: customChrome ? "transparent" : Theme.window
    // The compositor treats the shadow as outside the window, so the visible
    // frame can reach the screen edges.
    onShadowMarginChanged: backend.setWindowFrameMargin(shadowMargin)
    title: (backend.dirty ? "• " : "") + (backend.modName.length ? backend.modName : qsTr("Untitled mod")) + " – Stitchcraft"

    Backend { id: backend }
    BlockRows { id: blockRows; backend: backend }
    DevTools {
        id: dev
        backend: backend
        onJavaNeeded: devDialogs.askForJava()
        onEulaNeeded: loader => devDialogs.askForEula(loader)
        onFailed: message => backend.status = message
        onJarsReady: jars => backend.status = qsTr("Built %n jar(s). Save them from the console.", "", jars.length)
    }
    DevDialogs { id: devDialogs; dev: dev }

    // What Run and Debug start: a client, a server, or both, on one loader.
    property string runTarget: "client"
    property string runLoader: backend.fabric ? "fabric" : "neoforge"
    readonly property var enabledLoaders: (backend.fabric ? ["fabric"] : []).concat(backend.neoforge ? ["neoforge"] : [])
    onEnabledLoadersChanged: if (enabledLoaders.length && enabledLoaders.indexOf(runLoader) < 0) runLoader = enabledLoaders[0]
    readonly property string loaderLabel: runLoader === "fabric" ? "Fabric" : "NeoForge"
    readonly property string targetLabel: runTarget === "client" ? qsTr("client") : runTarget === "server" ? qsTr("server") : qsTr("client + server")
    property bool consoleOpen: false

    function run(debug) {
        consoleOpen = true;
        if (runTarget === "both") {
            if (dev.start("server", [runLoader], debug)) dev.start("client", [runLoader], debug);
        } else {
            dev.start(runTarget, [runLoader], debug);
        }
    }
    function buildJars() {
        consoleOpen = true;
        dev.start("build", enabledLoaders, false);
    }

    // A block's name for the debugger: its first label, or its type spelt out.
    function findInstruction(id) {
        function walk(list) {
            for (const ins of list || []) {
                if (ins.id === id) return ins;
                for (const key of ["body", "then_body", "else_body"]) {
                    const found = walk(ins[key]);
                    if (found) return found;
                }
            }
            return null;
        }
        for (const strand of document.strands || []) {
            const found = walk(strand.instructions);
            if (found) return found;
        }
        return null;
    }
    function blockTitle(id) {
        const ins = findInstruction(id);
        if (!ins) return qsTr("a block");
        const row = (catalog.rows || {})[ins.type];
        const label = row && (row.head || []).find(p => p.kind === "label");
        if (label) return label.text;
        return ins.type.replace(/([a-z0-9])([A-Z])/g, "$1 $2").toLowerCase();
    }
    function blockMenu() {
        return [{ id: "breakpoint", text: qsTr("Toggle breakpoint"), icon: "circle-dot",
                  when: (instruction, strandId, path) => !!instruction && !(path.length === 1 && path[0].index === 0) }];
    }

    // The document, parsed once per change rather than per binding.
    readonly property var document: JSON.parse(backend.documentJson || "{}")
    readonly property var catalog: blockRows.catalog
    readonly property var variableNames: (document.variables || []).map(v => v.name)

    // Runs one editor command. `onOk(result)` and `onError(message)` are
    // both optional; a refusal also shows in the status strip.
    function invoke(command, args, onOk, onError) {
        const answer = JSON.parse(backend.invoke(command, JSON.stringify(args || {})));
        if (answer.ok) { if (onOk) onOk(answer.result); }
        else if (onError) onError(answer.error);
        return answer;
    }

    // In-memory copy and paste: a fragment from copy_instructions.
    property var clipboard: null
    function copyBlocks(strandId, path, count, then) {
        invoke("copy_instructions", { strandId: strandId, path: path, count: count }, fragment => {
            window.clipboard = fragment;
            if (then) then();
        });
    }
    function removeInstruction(strandId, path) {
        invoke("remove_instruction", { strandId: strandId, path: path, count: 1 });
    }
    function blockDefById(id) {
        return (document.block_defs || []).find(d => d.id === id) || null;
    }
    function save() {
        if (backend.documentPath.length) backend.save("");
        else saveDialog.open();
    }
    function localPath(url) {
        const path = decodeURIComponent(url.toString().replace(/^file:\/\//, ""));
        // file:///C:/... on Windows
        return /^\/[A-Za-z]:\//.test(path) ? path.slice(1) : path;
    }

    Shortcut { sequences: [StandardKey.Undo]; onActivated: window.invoke("undo") }
    Shortcut { sequences: [StandardKey.Redo]; onActivated: window.invoke("redo") }
    Shortcut { sequences: [StandardKey.Save]; onActivated: window.save() }
    Shortcut { sequences: [StandardKey.Open]; onActivated: openDialog.open() }
    Shortcut { sequences: [StandardKey.New]; onActivated: backend.newProject() }

    // Pre-baked 9-slice shadow: scaling a bitmap is far cheaper per resize
    // frame than a live blur.
    BorderImage {
        visible: window.shadowMargin > 0
        anchors.fill: parent
        source: "qrc:/icons/window-shadow.png"
        border { left: 40; right: 40; top: 40; bottom: 40 }
    }
    // Everything inside the visible frame, clipped to its rounded corners.
    Item {
        id: frame
        anchors.fill: parent
        anchors.margins: window.shadowMargin
        layer.enabled: window.cornerRadius > 0
        layer.smooth: true
        layer.effect: MultiEffect { maskEnabled: true; maskSource: frameMask; maskThresholdMin: 0.5; maskSpreadAtMin: 1.0 }
        Rectangle { anchors.fill: parent; color: Theme.window }

        ColumnLayout {
            anchors.fill: parent
            spacing: 0

            // ── Toolbar ───────────────────────────────────────────────────────
            // Doubles as the title bar: its empty space moves the window.
            Rectangle {
                Layout.fillWidth: true
                Layout.preferredHeight: 52
                color: Theme.field
                border.color: Theme.borderSoft

                TitleBarDrag {
                    anchors.fill: parent
                    nativeButtons: macWindowButtons
                    // Every toolbar control except the logo and the spacer, which
                    // are where the window is grabbed.
                    interactiveItems: toolbarRow.children.filter(item => item !== toolbarLogo && item !== toolbarSpacer && item !== macWindowButtons)
                    windowButtons: windowButtons
                }

                RowLayout {
                    id: toolbarRow
                    anchors.fill: parent
                    anchors.leftMargin: 12
                    anchors.rightMargin: 12
                    spacing: 8

                    // Room for macOS's own traffic lights.
                    Item {
                        id: macWindowButtons
                        visible: Qt.platform.os === "osx"
                        Layout.preferredWidth: 72
                        Layout.preferredHeight: 34
                    }
                    Image {
                        id: toolbarLogo
                        source: "qrc:/icons/stitchcraft.png"
                        sourceSize: Qt.size(40, 40)
                        Layout.preferredWidth: 20
                        Layout.preferredHeight: 20
                        Layout.rightMargin: 2
                        fillMode: Image.PreserveAspectFit
                        smooth: true
                        mipmap: true
                    }
                    IconButton {
                        iconName: "panel-left"
                        tip: palette.visible ? qsTr("Hide sidebar") : qsTr("Show sidebar")
                        onClicked: palette.visible = !palette.visible
                    }
                    // The open mod: its name and id, and a click opens its settings.
                    Rectangle {
                        id: modButton
                        Layout.preferredWidth: Math.max(160, Math.min(280, window.width * 0.22))
                        Layout.preferredHeight: 34
                        radius: Theme.radius
                        color: modHover.hovered ? Theme.hover : Theme.panelRaised
                        border.color: Theme.border
                        Behavior on color { enabled: Motion.enabled; ColorAnimation { duration: Motion.quick } }
                        Accessible.role: Accessible.Button
                        Accessible.name: qsTr("Mod settings")
                        RowLayout {
                            anchors.fill: parent
                            anchors.leftMargin: 10
                            anchors.rightMargin: 8
                            spacing: 7
                            Text {
                                text: backend.modName.length ? backend.modName : qsTr("Untitled mod")
                                color: Theme.text
                                font.pixelSize: 13
                                elide: Text.ElideRight
                                Layout.maximumWidth: modButton.width * 0.6
                            }
                            Text {
                                Layout.fillWidth: true
                                text: backend.modId
                                color: Theme.textDim
                                font.pixelSize: 12
                                elide: Text.ElideRight
                            }
                            LucideIcon { name: "chevron-down"; width: 14; height: 14; color: Theme.textDim }
                        }
                        HoverHandler { id: modHover; cursorShape: Qt.PointingHandCursor }
                        TapHandler { onTapped: modSettings.openForEdit() }
                        ToolTip.visible: modHover.hovered
                        ToolTip.delay: 500
                        ToolTip.text: qsTr("Mod settings")
                    }
                    IconButton {
                        id: projectMenuButton
                        iconName: "ellipsis"
                        tip: projectMenu.visible ? "" : qsTr("Project")
                        property double pressStamp: 0
                        property double closeStamp: 0
                        onPressed: pressStamp = Date.now()
                        // The press that closes the menu also clicks the button.
                        onClicked: if (Math.abs(closeStamp - pressStamp) > 100) projectMenu.popup(projectMenuButton, 0, projectMenuButton.height + 4)
                        StitchMenu {
                            id: projectMenu
                            onAboutToHide: projectMenuButton.closeStamp = Date.now()
                            implicitWidth: 200
                            StitchMenuItem { iconName: "file-plus"; text: qsTr("New project"); onTriggered: backend.newProject() }
                            StitchMenuItem { iconName: "folder-open"; text: qsTr("Open…"); onTriggered: openDialog.open() }
                            StitchMenuItem { iconName: "save"; text: qsTr("Save as…"); onTriggered: saveDialog.open() }
                            StitchMenuItem { iconName: "sliders"; text: qsTr("Mod settings"); onTriggered: modSettings.openForEdit() }
                        }
                    }

                    Item { id: toolbarSpacer; Layout.fillWidth: true; Layout.fillHeight: true }

                    StitchButton {
                        implicitHeight: 34
                        iconName: "folder-output"
                        text: qsTr("Export source")
                        ToolTip.visible: hovered
                        ToolTip.delay: 500
                        ToolTip.text: qsTr("Write the mod's Gradle project to a folder, to keep or open in an IDE")
                        onClicked: exportDialog.open()
                    }
                    StitchButton {
                        implicitHeight: 34
                        iconName: "package"
                        text: dev.building ? qsTr("Building…") : qsTr("Build jars")
                        enabled: !dev.busy
                        ToolTip.visible: hovered
                        ToolTip.delay: 500
                        ToolTip.text: qsTr("Compile ready-to-install jars for %1").arg(window.enabledLoaders.map(l => l === "fabric" ? "Fabric" : "NeoForge").join(" and "))
                        onClicked: window.buildJars()
                    }
                    Rectangle { Layout.preferredWidth: 1; Layout.preferredHeight: 22; color: Theme.border; Layout.leftMargin: 2; Layout.rightMargin: 2 }

                    // Run and Debug share one target, picked from the chevron.
                    Row {
                        visible: !dev.runningGame
                        spacing: 0
                        StitchButton {
                            id: runButton
                            implicitHeight: 34
                            iconName: "play"
                            iconColor: Theme.running
                            text: qsTr("Run")
                            enabled: !dev.building
                            ToolTip.visible: hovered
                            ToolTip.delay: 500
                            ToolTip.text: qsTr("Start a %1 %2 with this mod in it").arg(window.loaderLabel).arg(window.targetLabel)
                            onClicked: window.run(false)
                        }
                        StitchButton {
                            implicitHeight: 34
                            iconName: "bug"
                            text: qsTr("Debug")
                            enabled: !dev.building
                            ToolTip.visible: hovered
                            ToolTip.delay: 500
                            ToolTip.text: qsTr("Run with block highlighting, breakpoints and live variables")
                            onClicked: window.run(true)
                        }
                        IconButton {
                            id: targetButton
                            iconName: "chevron-down"
                            tip: targetMenu.visible ? "" : qsTr("%1 %2").arg(window.loaderLabel).arg(window.targetLabel)
                            implicitWidth: 28
                            onClicked: targetMenu.popup(targetButton, 0, targetButton.height + 4)
                            StitchMenu {
                                id: targetMenu
                                implicitWidth: 230
                                StitchMenuItem { iconName: window.runTarget === "client" ? "check" : "blank"; text: qsTr("Client"); onTriggered: window.runTarget = "client" }
                                StitchMenuItem { iconName: window.runTarget === "server" ? "check" : "blank"; text: qsTr("Server"); onTriggered: window.runTarget = "server" }
                                StitchMenuItem { iconName: window.runTarget === "both" ? "check" : "blank"; text: qsTr("Client and server"); onTriggered: window.runTarget = "both" }
                                MenuSeparator { visible: window.enabledLoaders.length > 1; height: visible ? implicitHeight : 0 }
                                StitchMenuItem { visible: backend.fabric && backend.neoforge; height: visible ? implicitHeight : 0; iconName: window.runLoader === "fabric" ? "check" : "blank"; text: "Fabric"; onTriggered: window.runLoader = "fabric" }
                                StitchMenuItem { visible: backend.fabric && backend.neoforge; height: visible ? implicitHeight : 0; iconName: window.runLoader === "neoforge" ? "check" : "blank"; text: "NeoForge"; onTriggered: window.runLoader = "neoforge" }
                            }
                        }
                    }
                    StitchButton {
                        visible: dev.runningGame
                        implicitHeight: 34
                        iconName: "square"
                        danger: true
                        text: qsTr("Stop")
                        onClicked: dev.stopAll()
                    }
                    IconButton {
                        iconName: "terminal"
                        tip: window.consoleOpen ? qsTr("Hide the console") : qsTr("Show the console")
                        onClicked: window.consoleOpen = !window.consoleOpen
                    }

                    Item { Layout.fillWidth: true }

                    IconButton { iconName: "undo"; tip: qsTr("Undo"); enabled: backend.canUndo; onClicked: window.invoke("undo") }
                    IconButton { iconName: "redo"; tip: qsTr("Redo"); enabled: backend.canRedo; onClicked: window.invoke("redo") }
                    IconButton { iconName: "save"; tip: qsTr("Save project"); onClicked: window.save() }
                    IconButton { iconName: "settings"; tip: qsTr("Mod settings"); onClicked: modSettings.openForEdit() }
                    WindowControls {
                        id: windowButtons
                        visible: Qt.platform.os !== "osx"
                        Layout.leftMargin: 4
                    }
                }
            }

            // ── Palette and canvas ────────────────────────────────────────────
            Item {
                id: stage
                Layout.fillWidth: true
                Layout.fillHeight: true

                property real sidebarWidth: 360

                PalettePanel {
                    id: palette
                    anchors.left: parent.left
                    anchors.top: parent.top
                    anchors.bottom: parent.bottom
                    width: stage.sidebarWidth
                    z: 1
                    trashActive: canvas.dragging
                    trashArmed: canvas.dragging && palette.contains(palette.mapFromItem(null, canvas.dragSceneX, canvas.dragSceneY))
                    onResizeRequested: width => stage.sidebarWidth = Math.max(280, Math.min(560, width))
                    customBlockDetails: qsTr("A custom block defined in this mod.")
                    customReporterDetails: qsTr("A custom reporter defined in this mod.")

                    sectionsListBlocks: true
                    sections: window.catalog.sections || []
                    appCategories: window.catalog.categories || []
                    accordionCategory: "registry"
                    extraOperators: window.catalog.extraOperators || []
                    listInstructionTypes: window.catalog.listTypes || []
                    dictsEnabled: true
                    dictInstructionTypes: window.catalog.dictTypes || []

                    variables: window.variableNames
                    lists: window.document.lists || []
                    dicts: window.document.dicts || []
                    blockDefinitions: window.document.block_defs || []

                    onDragStarted: (spec, sx, sy, ox, oy) => drags.begin(spec, sx, sy, ox, oy)
                    onDragMoved: (sx, sy) => drags.move(sx, sy)
                    onDragEnded: (sx, sy) => drags.end(sx, sy)
                    onDragCanceled: drags.cancel()
                    onBlockActivated: (type, instruction) => drags.addAtView({ kind: "instruction", type: type, instruction: instruction })
                    onCustomBlockActivated: definition => drags.addAtView({ kind: "instruction", instruction: palette.customInstruction(definition) })
                    onValueActivated: value => drags.addAtView({ kind: "value", value: value })

                    onMakeVariableRequested: variableDialog.openForCreate()
                    onRenameVariableRequested: name => variableDialog.openForRename(name)
                    onDeleteVariableRequested: name => window.invoke("delete_variable", { name: name })
                    onMakeListRequested: listDialog.openForCreate()
                    onRenameListRequested: name => listDialog.openForRename(name)
                    onDeleteListRequested: name => window.invoke("delete_list", { name: name })
                    onListEditorStateRequested: (name, visible, x, y) => window.invoke("set_list_editor_state", { name: name, visible: visible, x: x, y: y })
                    onMakeDictRequested: dictDialog.openForCreate()
                    onRenameDictRequested: name => dictDialog.openForRename(name)
                    onDeleteDictRequested: name => window.invoke("delete_dict", { name: name })
                    onDictEditorStateRequested: (name, visible, x, y) => window.invoke("set_dict_editor_state", { name: name, visible: visible, x: x, y: y })
                    onMakeBlockRequested: blockDialog.openForCreate()
                    onEditBlockRequested: definition => blockDialog.openForEdit(definition)
                    onDeleteBlockRequested: blockId => window.invoke("delete_block", { blockId: blockId })
                }

                BlockCanvas {
                    id: canvas
                    anchors.left: palette.visible ? palette.right : parent.left
                    anchors.right: parent.right
                    anchors.top: parent.top
                    anchors.bottom: parent.bottom

                    strands: window.document.strands || []
                    comments: window.document.comments || []
                    floatingValues: window.document.floating_values || []
                    variables: window.variableNames
                    lists: window.document.lists || []
                    dicts: window.document.dicts || []
                    blockDefinitions: window.document.block_defs || []

                    canPaste: !!window.clipboard && window.clipboard.instructions.length > 0

                    // The debugger's marks: what just ran, where a game is
                    // paused, the breakpoints, and blocks that threw.
                    runningIds: dev.runningIds
                    pausedIds: dev.pausedIds
                    breakpointIds: dev.breakpoints
                    blockMarks: dev.failures
                    onBreakpointClicked: instructionId => dev.toggleBreakpoint(instructionId)
                    onMarkClicked: instructionId => { window.consoleOpen = true; }
                    clearText: qsTr("Delete all blocks")
                    onClearRequested: clearConfirm.open()
                    onCanvasMenuAction: (action, x, y) => {
                        if (action === "paste" && window.clipboard)
                            window.invoke("paste_fragment", { x: x, y: y, fragment: window.clipboard });
                    }
                    onBlockMenuAction: (action, strandId, path, instruction) => {
                        if (action === "edit_block") {
                            const def = window.blockDefById(instruction.block_id);
                            if (def) blockDialog.openForEdit(def);
                        } else if (action === "breakpoint") dev.toggleBreakpoint(instruction.id);
                        else if (action === "copy") window.copyBlocks(strandId, path, 1);
                        else if (action === "copy_all") window.copyBlocks(strandId, path, null);
                        else if (action === "cut") window.copyBlocks(strandId, path, 1, () => window.removeInstruction(strandId, path));
                    }

                    onStrandMoved: (strandId, x, y) => window.invoke("move_strand", { strandId: strandId, x: x, y: y })
                    onInstructionSplit: (strandId, path, x, y) => window.invoke("split_strand", { strandId: strandId, path: path, x: x, y: y })
                    onStrandsMerged: (draggedId, targetId, path, targetY) => window.invoke("merge_strand", { draggedId: draggedId, targetId: targetId, path: path, targetY: targetY })
                    onTailMerged: (strandId, path, targetId, targetPath, targetY) => window.invoke("merge_tail", { strandId: strandId, path: path, targetId: targetId, targetPath: targetPath, targetY: targetY })
                    onInstructionRemoved: (strandId, path) => window.removeInstruction(strandId, path)
                    onInstructionDuplicated: (strandId, path) => window.invoke("duplicate_instruction", { strandId: strandId, path: path })
                    onInstructionEdited: (strandId, path, instruction) => window.invoke("edit_instruction", { strandId: strandId, path: path, instruction: instruction })

                    onValueEdited: (location, text) => window.invoke("edit_value_field", { location: location, text: text })
                    onValueTakeRequested: location => window.invoke("take_value", { location: location })
                    onValuePutRequested: (location, value) => window.invoke("put_value", { location: location, value: value })
                    onValueCreateRequested: (x, y, value, originBlockId) => window.invoke("create_floating_value", { x: x, y: y, value: value, originBlockId: originBlockId || null })
                    onValueMoveRequested: (from, to, x, y, originBlockId) => window.invoke("move_value", { from: from, to: to, x: x, y: y, originBlockId: originBlockId || null })
                    onFloatingValueMoved: (floatingId, x, y) => window.invoke("move_floating_value", { floatingId: floatingId, x: x, y: y })
                    onFloatingValueRemoved: floatingId => window.invoke("remove_floating_value", { floatingId: floatingId })

                    onCanvasNoteRequested: (x, y) => window.invoke("create_comment", { x: x, y: y, text: "" })
                    onCommentForInstructionRequested: instruction => {
                        const existing = (window.document.comments || []).find(c => c.attached_to === instruction.id);
                        if (existing) { if (existing.collapsed) window.invoke("set_comment_collapsed", { commentId: existing.id, collapsed: false }); }
                        else window.invoke("create_attached_comment", { instructionId: instruction.id, dx: 48, dy: 18, text: "" });
                    }
                    onCommentMoved: (commentId, x, y) => window.invoke("move_comment", { commentId: commentId, x: x, y: y })
                    onCommentEdited: (commentId, text) => window.invoke("edit_comment_text", { commentId: commentId, text: text })
                    onCommentCollapseChanged: (commentId, collapsed) => window.invoke("set_comment_collapsed", { commentId: commentId, collapsed: collapsed })
                    onCommentRemoved: commentId => window.invoke("remove_comment", { commentId: commentId })

                    onListItemsEdited: (name, items) => window.invoke("set_list_items", { name: name, items: items })
                    onListEditorStateChanged: (name, visible, x, y) => window.invoke("set_list_editor_state", { name: name, visible: visible, x: x, y: y })
                    onDictEntriesEdited: (name, entries) => window.invoke("set_dict_entries", { name: name, entries: entries })
                    onDictEditorStateChanged: (name, visible, x, y) => window.invoke("set_dict_editor_state", { name: name, visible: visible, x: x, y: y })
                }

                // Palette drags and drops on the sidebar's trash. Each one is one
                // edit, named the way the backend's commands are.
                PaletteDragController {
                    id: drags
                    anchors.fill: parent
                    canvas: canvas
                    variables: window.variableNames
                    lists: window.document.lists || []
                    blockDefinitions: window.document.block_defs || []
                    documentId: backend.documentPath
                    document: [window.document.strands, window.document.floating_values]
                    trashContains: (sx, sy) => palette.visible && palette.contains(palette.mapFromItem(null, sx, sy))
                    submit: (edit, reply) => window.invoke(edit.kind, edit, () => reply(true), error => reply(false, error))
                }
            }

            ConsolePanel {
                id: consolePanel
                visible: window.consoleOpen
                Layout.fillWidth: true
                Layout.preferredHeight: panelHeight
                dev: dev
                blockTitle: id => window.blockTitle(id)
                onRevealRequested: id => canvas.revealInstruction(id)
                onSaveJarsRequested: jars => { jarsDialog.jars = jars; jarsDialog.open(); }
                onOpenFolderRequested: path => Qt.openUrlExternally("file://" + (path.startsWith("/") ? "" : "/") + path)
                onHideRequested: window.consoleOpen = false
            }

            DiagnosticsBar {
                Layout.fillWidth: true
                Layout.preferredHeight: implicitHeight
                backend: backend
                onStrandRequested: strandId => {
                    const strand = (window.document.strands || []).find(s => s.id === strandId);
                    if (strand && strand.instructions.length) canvas.revealInstruction(strand.instructions[0].id);
                }
            }
        }

    }
    Item {
        id: frameMask
        anchors.fill: frame
        visible: false
        layer.enabled: window.cornerRadius > 0
        layer.smooth: true
        Rectangle { anchors.fill: parent; radius: window.cornerRadius; color: "black"; antialiasing: true }
    }
    // The frameless window's hairline outline and edge/corner resize grips.
    Rectangle {
        visible: window.customChrome && window.visibility !== Window.Maximized
        anchors.fill: parent
        anchors.margins: window.shadowMargin
        color: "transparent"
        radius: window.cornerRadius
        border.color: Theme.border
        z: 150
    }
    Repeater {
        model: window.customChrome && window.visibility === Window.Windowed ? [
            { e: Qt.TopEdge, c: Qt.SizeVerCursor }, { e: Qt.BottomEdge, c: Qt.SizeVerCursor },
            { e: Qt.LeftEdge, c: Qt.SizeHorCursor }, { e: Qt.RightEdge, c: Qt.SizeHorCursor },
            { e: Qt.TopEdge | Qt.LeftEdge, c: Qt.SizeFDiagCursor }, { e: Qt.BottomEdge | Qt.RightEdge, c: Qt.SizeFDiagCursor },
            { e: Qt.TopEdge | Qt.RightEdge, c: Qt.SizeBDiagCursor }, { e: Qt.BottomEdge | Qt.LeftEdge, c: Qt.SizeBDiagCursor }
        ] : []
        delegate: MouseArea {
            required property var modelData
            readonly property int grip: 7
            readonly property bool edgeL: (modelData.e & Qt.LeftEdge) !== 0
            readonly property bool edgeR: (modelData.e & Qt.RightEdge) !== 0
            readonly property bool edgeT: (modelData.e & Qt.TopEdge) !== 0
            readonly property bool edgeB: (modelData.e & Qt.BottomEdge) !== 0
            readonly property bool corner: (edgeL || edgeR) && (edgeT || edgeB)
            z: 200
            cursorShape: modelData.c
            x: edgeR ? window.width - window.shadowMargin - width : window.shadowMargin
            y: edgeB ? window.height - window.shadowMargin - height : window.shadowMargin
            width: (edgeL || edgeR) ? (corner ? grip * 2 : grip) : window.width - 2 * window.shadowMargin
            height: (edgeT || edgeB) ? (corner ? grip * 2 : grip) : window.height - 2 * window.shadowMargin
            onPressed: window.startSystemResize(modelData.e)
        }
    }

    // ── Dialogs ───────────────────────────────────────────────────────────
    ModSettingsDialog { id: modSettings; backend: backend }
    MakeBlockDialog { id: blockDialog; invoke: window.invoke }

    NameDialog {
        id: variableDialog
        noun: "variable"
        onSubmitted: (name, renameTarget) => renameTarget.length
            ? window.invoke("rename_variable", { oldName: renameTarget, newName: name }, () => close(), e => fail(e))
            : window.invoke("create_variable", { name: name }, () => close(), e => fail(e))
    }
    NameDialog {
        id: listDialog
        noun: "list"
        onSubmitted: (name, renameTarget) => renameTarget.length
            ? window.invoke("rename_list", { oldName: renameTarget, newName: name }, () => close(), e => fail(e))
            : window.invoke("create_list", { name: name }, () => close(), e => fail(e))
    }
    NameDialog {
        id: dictDialog
        noun: "dict"
        onSubmitted: (name, renameTarget) => renameTarget.length
            ? window.invoke("rename_dict", { oldName: renameTarget, newName: name }, () => close(), e => fail(e))
            : window.invoke("create_dict", { name: name }, () => close(), e => fail(e))
    }

    StitchDialog {
        id: clearConfirm
        title: qsTr("Delete all blocks?")
        standardButtons: Dialog.NoButton
        Column {
            spacing: 18
            Text {
                width: 340
                wrapMode: Text.WordWrap
                text: qsTr("Every stack on the canvas is removed. Undo brings them back.")
                color: Theme.text
                font.pixelSize: 13
            }
            Row {
                anchors.right: parent.right
                spacing: 8
                StitchButton { text: qsTr("Cancel"); onClicked: clearConfirm.reject() }
                StitchButton { text: qsTr("Delete"); danger: true; onClicked: { window.invoke("clear_instructions"); clearConfirm.accept(); } }
            }
        }
    }

    FileDialog {
        id: openDialog
        title: qsTr("Open a Stitchcraft project")
        nameFilters: [qsTr("Stitchcraft project (*.stitch)"), qsTr("All files (*)")]
        onAccepted: backend.open(window.localPath(selectedFile))
    }
    FileDialog {
        id: saveDialog
        title: qsTr("Save the project")
        fileMode: FileDialog.SaveFile
        defaultSuffix: "stitch"
        nameFilters: [qsTr("Stitchcraft project (*.stitch)")]
        onAccepted: backend.save(window.localPath(selectedFile))
    }
    FolderDialog {
        id: jarsDialog
        property var jars: []
        title: qsTr("Save the jars into")
        onAccepted: {
            const answer = dev.call("copyJars", { jars: jars, directory: window.localPath(selectedFolder) });
            backend.status = answer.ok ? qsTr("Saved %n jar(s).", "", jars.length) : answer.error;
        }
    }
    FolderDialog {
        id: exportDialog
        title: qsTr("Export the mod into")
        onAccepted: backend.exportMod(window.localPath(selectedFolder))
    }

    Component.onCompleted: {
        backend.setWindowFrameMargin(shadowMargin);
        BlockRegistry.blockMenu = window.blockMenu();
        // A brand new project's id is a placeholder, and every registered
        // block is named under it, so ask first.
        if (!backend.documentPath.length && backend.modId === "example_mod") modSettings.openForEdit();
    }
}
