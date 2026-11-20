import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.blockworked.Blockstitch

// The dock under the canvas for builds and development runs: one tab per
// job with its Gradle and game output, a line to type server commands into,
// and, while a debug run is connected, the debugger's controls and the
// canvas's variables.
Rectangle {
    id: root

    required property var dev
    // Answers a block's name from its instruction id, for "Paused before ...".
    property var blockTitle: id => id
    property real panelHeight: 260
    signal revealRequested(string instructionId)
    signal saveJarsRequested(var jars)
    signal openFolderRequested(string path)
    signal hideRequested()

    readonly property var job: dev.current
    readonly property var model: job ? dev.consoles[job.id] || null : null
    readonly property bool showDebugger: !!job && job.debug && job.action !== "build"

    color: Theme.panel
    border.color: Theme.borderSoft
    clip: true

    // Drag the top edge to resize.
    Rectangle {
        anchors.left: parent.left
        anchors.right: parent.right
        height: 5
        z: 2
        color: resizeArea.containsMouse || resizeArea.pressed ? Theme.accent : "transparent"
        opacity: 0.6
        MouseArea {
            id: resizeArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.SizeVerCursor
            property real startY: 0
            property real startHeight: 0
            onPressed: mouse => { startY = mapToItem(null, 0, mouse.y).y; startHeight = root.panelHeight; }
            onPositionChanged: mouse => {
                if (!pressed) return;
                const y = mapToItem(null, 0, mouse.y).y;
                root.panelHeight = Math.max(140, Math.min(640, startHeight + startY - y));
            }
        }
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        // ── Tabs and actions ─────────────────────────────────────────────
        RowLayout {
            Layout.fillWidth: true
            Layout.preferredHeight: 36
            Layout.leftMargin: 8
            Layout.rightMargin: 6
            spacing: 2

            Repeater {
                model: root.dev.jobs
                delegate: Rectangle {
                    id: tab
                    required property var modelData
                    readonly property bool selected: modelData.id === root.dev.currentJob
                    Layout.preferredHeight: 28
                    Layout.preferredWidth: tabRow.implicitWidth + 20
                    radius: Theme.radius
                    color: selected ? Theme.field : tabHover.hovered ? Theme.hover : "transparent"
                    border.color: selected ? Theme.border : "transparent"
                    RowLayout {
                        id: tabRow
                        anchors.centerIn: parent
                        spacing: 7
                        Rectangle {
                            width: 8; height: 8; radius: 4
                            color: tab.modelData.running ? Theme.accent
                                 : tab.modelData.ok ? Theme.running
                                 : tab.modelData.stopped ? Theme.textDim : Theme.danger
                            SequentialAnimation on opacity {
                                running: tab.modelData.running && Motion.enabled
                                loops: Animation.Infinite
                                NumberAnimation { to: 0.35; duration: 600 }
                                NumberAnimation { to: 1; duration: 600 }
                                onRunningChanged: if (!running) parent.opacity = 1
                            }
                        }
                        LucideIcon {
                            visible: tab.modelData.debug
                            name: "bug"; width: 13; height: 13; color: Theme.textDim
                        }
                        Text {
                            text: tab.modelData.title
                            color: tab.selected ? Theme.text : Theme.textDim
                            font.pixelSize: 12
                            font.weight: tab.selected ? Font.DemiBold : Font.Normal
                        }
                        LucideIcon {
                            visible: !tab.modelData.running
                            name: "x"; width: 13; height: 13
                            color: closeHover.hovered ? Theme.text : Theme.textDim
                            HoverHandler { id: closeHover; cursorShape: Qt.PointingHandCursor }
                            TapHandler { onTapped: root.dev.closeJob(tab.modelData.id) }
                        }
                    }
                    HoverHandler { id: tabHover }
                    TapHandler { onTapped: root.dev.currentJob = tab.modelData.id }
                }
            }
            Text {
                visible: root.dev.jobs.length === 0
                text: qsTr("Builds and test runs show their output here.")
                color: Theme.textDim
                font.pixelSize: 12
                Layout.leftMargin: 4
            }
            Item { Layout.fillWidth: true }

            StitchButton {
                visible: !!root.job && root.job.running
                implicitHeight: 28
                iconName: "square"
                text: root.job && root.job.stopped ? qsTr("Force stop") : qsTr("Stop")
                danger: true
                onClicked: root.dev.stop(root.job.id)
            }
            IconButton {
                iconName: "copy"
                tip: qsTr("Copy the output")
                enabled: !!root.model && root.model.count > 0
                implicitWidth: 30; implicitHeight: 30
                onClicked: {
                    const lines = [];
                    for (let i = 0; i < root.model.count; i++) lines.push(root.model.get(i).line);
                    clipboardHelper.text = lines.join("\n");
                    clipboardHelper.selectAll();
                    clipboardHelper.copy();
                }
            }
            IconButton {
                iconName: "folder-open"
                tip: qsTr("Open the build folder")
                implicitWidth: 30; implicitHeight: 30
                onClicked: root.openFolderRequested(root.dev.workspacePath())
            }
            IconButton {
                iconName: "chevron-down"
                tip: qsTr("Hide")
                implicitWidth: 30; implicitHeight: 30
                onClicked: root.hideRequested()
            }
        }
        Rectangle { Layout.fillWidth: true; Layout.preferredHeight: 1; color: Theme.borderSoft }

        // Built jars, ready to take.
        Rectangle {
            visible: !!root.job && !root.job.running && root.job.jars.length > 0
            Layout.fillWidth: true
            Layout.preferredHeight: 40
            color: Theme.field
            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 12
                anchors.rightMargin: 8
                spacing: 8
                LucideIcon { name: "package-check"; width: 16; height: 16; color: Theme.running }
                Text {
                    Layout.fillWidth: true
                    elide: Text.ElideMiddle
                    color: Theme.text
                    font.pixelSize: 12
                    text: root.job ? root.job.jars.map(j => j.split(/[\\/]/).pop()).join("   ") : ""
                }
                StitchButton {
                    implicitHeight: 28
                    iconName: "download"
                    text: qsTr("Save jars…")
                    onClicked: root.saveJarsRequested(root.job.jars)
                }
            }
        }

        RowLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 0

            // ── Output ───────────────────────────────────────────────────
            ColumnLayout {
                Layout.fillWidth: true
                Layout.fillHeight: true
                spacing: 0

                StitchListView {
                    id: lines
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    Layout.leftMargin: 10
                    Layout.topMargin: 4
                    clip: true
                    model: root.model
                    // Follow the end unless the user scrolled up to read.
                    property bool follow: true
                    onMovementEnded: follow = atYEnd
                    onCountChanged: if (follow) Qt.callLater(positionViewAtEnd)
                    delegate: Text {
                        required property string line
                        width: lines.width - 14
                        text: line.length ? line : " "
                        textFormat: Text.PlainText
                        wrapMode: Text.WrapAnywhere
                        font.family: "monospace"
                        font.pixelSize: 12
                        lineHeight: 1.15
                        color: /\b(ERROR|FATAL|Exception|FAILED|error:)/.test(line) ? Theme.danger
                             : /\b(WARN|warning:)/.test(line) ? Theme.warning
                             : line.startsWith("> ") ? Theme.accent : Theme.text
                    }
                }

                // Typing into the game, like a server's console.
                RowLayout {
                    visible: !!root.job && root.job.running && root.job.action !== "build"
                    Layout.fillWidth: true
                    Layout.margins: 6
                    spacing: 6
                    StitchTextField {
                        id: input
                        Layout.fillWidth: true
                        placeholderText: root.job && root.job.action === "server"
                            ? qsTr("Type a server command, like  time set day")
                            : qsTr("Type into the game's console")
                        font.family: "monospace"
                        onAccepted: {
                            if (!text.trim().length) return;
                            root.dev.sendInput(root.job.id, text);
                            text = "";
                        }
                    }
                }
            }

            // ── Debugger ─────────────────────────────────────────────────
            Rectangle {
                visible: root.showDebugger
                Layout.preferredWidth: 300
                Layout.fillHeight: true
                color: Theme.panelRaised
                border.color: Theme.borderSoft

                ColumnLayout {
                    anchors.fill: parent
                    anchors.margins: 10
                    spacing: 8

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 8
                        LucideIcon {
                            name: root.dev.paused ? "pause" : root.dev.debugging ? "play" : "plug"
                            width: 16; height: 16
                            color: root.dev.paused ? Theme.warning : root.dev.debugging ? Theme.running : Theme.textDim
                        }
                        Text {
                            Layout.fillWidth: true
                            elide: Text.ElideRight
                            color: Theme.text
                            font.pixelSize: 13
                            font.weight: Font.DemiBold
                            text: root.dev.paused ? qsTr("Paused before “%1”").arg(root.blockTitle(root.dev.paused.id))
                                : root.dev.debugging ? qsTr("Running, %1 connected").arg(root.dev.sides.join(" and "))
                                : root.job && root.job.running ? qsTr("Waiting for the game…") : qsTr("Not connected")
                            HoverHandler { cursorShape: root.dev.paused ? Qt.PointingHandCursor : Qt.ArrowCursor }
                            TapHandler { enabled: !!root.dev.paused; onTapped: root.revealRequested(root.dev.paused.id) }
                        }
                    }
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 6
                        StitchButton {
                            visible: !root.dev.paused
                            enabled: root.dev.debugging
                            implicitHeight: 28
                            iconName: "pause"
                            text: qsTr("Pause")
                            ToolTip.visible: hovered
                            ToolTip.delay: 400
                            ToolTip.text: qsTr("Pause before the next block that runs (F8)")
                            onClicked: root.dev.debugCommand("pause")
                        }
                        StitchButton {
                            visible: !!root.dev.paused
                            implicitHeight: 28
                            iconName: "play"
                            text: qsTr("Continue")
                            ToolTip.visible: hovered
                            ToolTip.delay: 400
                            ToolTip.text: qsTr("Run on until the next breakpoint (F8)")
                            onClicked: root.dev.debugCommand("continue")
                        }
                        StitchButton {
                            visible: !!root.dev.paused
                            implicitHeight: 28
                            iconName: "step-forward"
                            text: qsTr("Step")
                            ToolTip.visible: hovered
                            ToolTip.delay: 400
                            ToolTip.text: qsTr("Run one block, then pause again (F10)")
                            onClicked: root.dev.debugCommand("step")
                        }
                        Item { Layout.fillWidth: true }
                        IconButton {
                            iconName: "circle-off"
                            tip: qsTr("Clear breakpoints")
                            enabled: root.dev.breakpoints.length > 0
                            implicitWidth: 28; implicitHeight: 28
                            onClicked: root.dev.clearBreakpoints()
                        }
                    }

                    Text {
                        text: qsTr("Variables")
                        color: Theme.textDim
                        font.pixelSize: 12
                        font.weight: Font.DemiBold
                        Layout.topMargin: 4
                    }
                    StitchListView {
                        id: varList
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        clip: true
                        spacing: 2
                        model: {
                            const v = root.dev.vars;
                            if (!v) return [];
                            const out = [];
                            for (const name in v.variables || {}) out.push({ name: name, value: JSON.stringify(v.variables[name]) });
                            for (const name in v.lists || {}) out.push({ name: name, value: JSON.stringify(v.lists[name]) });
                            for (const name in v.dicts || {}) out.push({ name: name, value: JSON.stringify(v.dicts[name]) });
                            return out;
                        }
                        delegate: RowLayout {
                            required property var modelData
                            width: varList.width
                            spacing: 8
                            Text {
                                text: parent.modelData.name
                                color: Theme.textDim
                                font.pixelSize: 12
                                elide: Text.ElideRight
                                Layout.preferredWidth: Math.min(implicitWidth, varList.width * 0.45)
                            }
                            Text {
                                Layout.fillWidth: true
                                text: parent.modelData.value
                                color: Theme.text
                                font.family: "monospace"
                                font.pixelSize: 12
                                elide: Text.ElideRight
                            }
                        }
                        Text {
                            visible: varList.count === 0
                            text: root.dev.debugging ? qsTr("This mod has no variables.") : qsTr("Shown once the game connects.")
                            color: Theme.textDim
                            font.pixelSize: 12
                        }
                    }

                    // Blocks that threw, newest last.
                    Repeater {
                        model: Object.keys(root.dev.failures)
                        delegate: Rectangle {
                            required property string modelData
                            Layout.fillWidth: true
                            Layout.preferredHeight: failText.implicitHeight + 12
                            radius: Theme.radius
                            color: Theme.field
                            border.color: Theme.danger
                            RowLayout {
                                id: failRow
                                anchors.fill: parent
                                anchors.margins: 6
                                spacing: 6
                                LucideIcon { name: "circle-alert"; width: 14; height: 14; color: Theme.danger; Layout.alignment: Qt.AlignTop }
                                Text {
                                    id: failText
                                    Layout.fillWidth: true
                                    wrapMode: Text.Wrap
                                    font.pixelSize: 12
                                    color: Theme.text
                                    text: qsTr("“%1” failed: %2").arg(root.blockTitle(parent.parent.modelData)).arg(root.dev.failures[parent.parent.modelData])
                                }
                            }
                            HoverHandler { cursorShape: Qt.PointingHandCursor }
                            TapHandler { onTapped: root.revealRequested(parent.modelData) }
                        }
                    }
                }
            }
        }
    }

    TextEdit { id: clipboardHelper; visible: false }

    Shortcut { sequence: "F8"; enabled: root.dev.debugging; onActivated: root.dev.debugCommand(root.dev.paused ? "continue" : "pause") }
    Shortcut { sequence: "F10"; enabled: !!root.dev.paused; onActivated: root.dev.debugCommand("step") }
}
