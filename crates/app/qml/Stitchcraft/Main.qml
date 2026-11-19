import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs
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

    width: 1440
    height: 900
    minimumWidth: 900
    minimumHeight: 560
    visible: true
    color: Theme.window
    title: (backend.dirty ? "• " : "") + (backend.modName.length ? backend.modName : qsTr("Untitled mod")) + " – Stitchcraft"

    Backend { id: backend }
    BlockRows { id: blockRows; backend: backend }

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

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        // ── Toolbar ───────────────────────────────────────────────────────
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 52
            color: Theme.field
            border.color: Theme.borderSoft

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 12
                anchors.rightMargin: 12
                spacing: 8

                Image {
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

                Item { Layout.fillWidth: true }

                StitchButton {
                    implicitHeight: 34
                    iconName: "download"
                    iconColor: Theme.accent
                    text: qsTr("Export mod")
                    ToolTip.visible: hovered
                    ToolTip.delay: 500
                    ToolTip.text: (backend.fabric ? "Fabric" : "") + (backend.fabric && backend.neoforge ? " + " : "") + (backend.neoforge ? "NeoForge" : "")
                    onClicked: exportDialog.open()
                }

                Item { Layout.fillWidth: true }

                IconButton { iconName: "undo"; tip: qsTr("Undo"); enabled: backend.canUndo; onClicked: window.invoke("undo") }
                IconButton { iconName: "redo"; tip: qsTr("Redo"); enabled: backend.canRedo; onClicked: window.invoke("redo") }
                IconButton { iconName: "save"; tip: qsTr("Save project"); onClicked: window.save() }
                IconButton { iconName: "settings"; tip: qsTr("Mod settings"); onClicked: modSettings.openForEdit() }
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
                    } else if (action === "copy") window.copyBlocks(strandId, path, 1);
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
        id: exportDialog
        title: qsTr("Export the mod into")
        onAccepted: backend.exportMod(window.localPath(selectedFolder))
    }

    Component.onCompleted: {
        // A brand new project's id is a placeholder, and every registered
        // block is named under it, so ask first.
        if (!backend.documentPath.length && backend.modId === "example_mod") modSettings.openForEdit();
    }
}
