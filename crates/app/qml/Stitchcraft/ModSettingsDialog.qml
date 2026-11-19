import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.blockworked.Blockstitch

// The mod's identity: what goes in `fabric.mod.json`, `neoforge.mods.toml`
// and the namespace every registered id gets.
//
// The mod id is the one field the rest of the export depends on the shape
// of, so the backend sanitizes it on the way through.
StitchDialog {
    id: root

    required property var backend

    title: qsTr("Mod settings")
    width: 460
    standardButtons: Dialog.NoButton
    closePolicy: Popup.CloseOnEscape

    function openForEdit() {
        idField.text = root.backend.modId;
        nameField.text = root.backend.modName;
        versionField.text = root.backend.modVersion;
        packageField.text = root.backend.modPackage;
        fabricSwitch.checked = root.backend.fabric;
        neoforgeSwitch.checked = root.backend.neoforge;
        open();
        idField.forceActiveFocus();
    }

    onAccepted: backend.setMetadata(idField.text, nameField.text, versionField.text,
                                    packageField.text, fabricSwitch.checked, neoforgeSwitch.checked)

    component Label: Text {
        color: Theme.text
        font.pixelSize: 13
        font.weight: Font.DemiBold
        Layout.topMargin: 6
    }
    component Hint: Text {
        Layout.fillWidth: true
        wrapMode: Text.WordWrap
        color: Theme.textDim
        font.pixelSize: 12
    }

    ColumnLayout {
        width: parent.width
        spacing: 6

        Label { text: qsTr("Mod id"); Layout.topMargin: 0 }
        StitchTextField { id: idField; Layout.fillWidth: true; placeholderText: "wonder_blocks"; onAccepted: root.accept() }
        Hint { text: qsTr("Lowercase letters, digits and underscores. Everything the canvas registers is named under it.") }

        Label { text: qsTr("Display name") }
        StitchTextField { id: nameField; Layout.fillWidth: true; placeholderText: "Wonder Blocks"; onAccepted: root.accept() }

        RowLayout {
            Layout.fillWidth: true
            spacing: 12
            ColumnLayout {
                Layout.preferredWidth: 120
                spacing: 6
                Label { text: qsTr("Version") }
                StitchTextField { id: versionField; Layout.fillWidth: true; placeholderText: "1.0.0"; onAccepted: root.accept() }
            }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 6
                Label { text: qsTr("Java package") }
                StitchTextField { id: packageField; Layout.fillWidth: true; placeholderText: "com.example.wonder_blocks"; onAccepted: root.accept() }
            }
        }

        Label { text: qsTr("Build for") }
        RowLayout {
            spacing: 20
            RowLayout {
                spacing: 8
                StitchSwitch { id: fabricSwitch }
                Text { text: "Fabric"; color: Theme.text; font.pixelSize: 13 }
            }
            RowLayout {
                spacing: 8
                StitchSwitch { id: neoforgeSwitch }
                Text { text: "NeoForge"; color: Theme.text; font.pixelSize: 13 }
            }
        }

        RowLayout {
            Layout.fillWidth: true
            Layout.topMargin: 14
            spacing: 8
            Item { Layout.fillWidth: true }
            StitchButton { text: qsTr("Cancel"); onClicked: root.reject() }
            StitchButton { text: qsTr("Save"); primary: true; onClicked: root.accept() }
        }
    }
}
