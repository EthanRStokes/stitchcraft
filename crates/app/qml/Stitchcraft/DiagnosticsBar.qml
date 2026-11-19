import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.blockworked.Blockstitch

// Status strip under the canvas: whether what's on it can be exported, and
// the last thing the editor said. Opens into the list of problems.
//
// Errors stop an export; warnings compile but won't do what the blocks say.
// Both come from `stitchcraft_blocks::validate`, so the editor and the
// exporter never disagree.
Rectangle {
    id: root

    required property var backend
    property bool expanded: false
    property real panelHeight: 200
    readonly property int stripHeight: 30
    // Clicking a problem asks the window to show its stack.
    signal strandRequested(string strandId)

    readonly property var diagnostics: JSON.parse(backend.diagnosticsJson || "[]")
    readonly property var errors: diagnostics.filter(d => d.severity === "error")
    readonly property var warnings: diagnostics.filter(d => d.severity === "warning")
    readonly property color stateColor: errors.length ? Theme.danger : warnings.length ? Theme.warning : Theme.running

    implicitHeight: expanded && diagnostics.length ? panelHeight : stripHeight
    Behavior on implicitHeight {
        enabled: Motion.enabled
        NumberAnimation { duration: Motion.normal; easing.type: Easing.OutCubic }
    }
    readonly property bool open: height > stripHeight + 0.5
    color: Theme.panel
    border.color: Theme.borderSoft
    clip: true

    function summary() {
        if (errors.length) return qsTr("%n problem(s) to fix before exporting", "", errors.length);
        if (warnings.length) return qsTr("Ready to export, with %n warning(s)", "", warnings.length);
        return qsTr("Ready to export");
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0
        Item {
            Layout.fillWidth: true
            Layout.preferredHeight: root.stripHeight
            MouseArea {
                anchors.fill: parent
                enabled: root.diagnostics.length > 0
                cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
                onClicked: root.expanded = !root.expanded
            }
            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 12
                anchors.rightMargin: 8
                spacing: 8
                Rectangle { width: 8; height: 8; radius: 4; color: root.stateColor }
                Text {
                    text: root.summary()
                    color: root.errors.length ? Theme.danger : Theme.textDim
                    font.pixelSize: 12
                }
                Text {
                    Layout.fillWidth: true
                    text: root.backend.status
                    textFormat: Text.PlainText
                    color: Theme.textDim
                    font.pixelSize: 12
                    elide: Text.ElideRight
                    visible: text.length > 0
                }
                Item { Layout.fillWidth: true; visible: !root.backend.status.length }
                LucideIcon {
                    visible: root.diagnostics.length > 0
                    name: "chevron-up"
                    rotation: root.expanded ? 180 : 0
                    color: Theme.textDim
                    width: 16
                    height: 16
                    Behavior on rotation { enabled: Motion.enabled; NumberAnimation { duration: Motion.normal; easing.type: Easing.OutCubic } }
                }
            }
        }
        Rectangle { visible: root.open; Layout.fillWidth: true; Layout.preferredHeight: 1; color: Theme.borderSoft }
        StitchListView {
            id: list
            visible: root.open
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.margins: 4
            clip: true
            spacing: 1
            model: root.diagnostics
            delegate: Rectangle {
                id: row
                required property var modelData
                readonly property bool error: modelData.severity === "error"
                readonly property bool linked: !!modelData.strandId
                width: list.width
                height: line.implicitHeight + 8
                radius: 4
                color: rowHover.hovered && linked ? Theme.hover : "transparent"
                HoverHandler { id: rowHover; cursorShape: row.linked ? Qt.PointingHandCursor : Qt.ArrowCursor }
                TapHandler { enabled: row.linked; onTapped: root.strandRequested(row.modelData.strandId) }
                RowLayout {
                    id: line
                    x: 8
                    width: parent.width - 16
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 10
                    LucideIcon {
                        name: row.error ? "circle-alert" : "triangle-alert"
                        color: row.error ? Theme.danger : Theme.warning
                        width: 14
                        height: 14
                        Layout.alignment: Qt.AlignTop
                    }
                    Text {
                        Layout.fillWidth: true
                        text: row.modelData.message
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                        color: Theme.text
                        font.pixelSize: 12
                    }
                    LucideIcon { visible: row.linked; name: "crosshair"; color: Theme.textDim; width: 14; height: 14; Layout.alignment: Qt.AlignTop }
                }
            }
        }
    }
}
