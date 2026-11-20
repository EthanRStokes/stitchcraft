import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.blockworked.Blockstitch

// The two things the dev tools may have to ask before they can start:
// getting Java, and agreeing to the Minecraft EULA before a server runs.
Item {
    id: root

    required property var dev

    function askForJava() { javaDialog.open(); }
    function askForEula(loader) { eulaDialog.loader = loader; eulaDialog.open(); }

    component Body: Text {
        Layout.fillWidth: true
        wrapMode: Text.WordWrap
        color: Theme.text
        font.pixelSize: 13
        lineHeight: 1.2
    }

    StitchDialog {
        id: javaDialog
        title: root.dev.java.downloading ? qsTr("Getting Java…") : qsTr("Java is needed")
        width: 440
        standardButtons: Dialog.NoButton
        closePolicy: Popup.CloseOnEscape

        ColumnLayout {
            width: parent.width
            spacing: 14
            Body {
                text: root.dev.java.downloading
                    ? qsTr("Downloading Eclipse Temurin 25. Your build starts by itself once it's ready.")
                    : qsTr("Minecraft mods are built and tested with Java. Stitchcraft can download Eclipse Temurin 25 (about 140 MB) for its own use, or you can install a JDK 17 or newer yourself and restart.")
            }
            Rectangle {
                visible: root.dev.java.downloading
                Layout.fillWidth: true
                Layout.preferredHeight: 6
                radius: 3
                color: Theme.field
                border.color: Theme.borderSoft
                Rectangle {
                    width: parent.width * Math.max(0.03, root.dev.javaProgress)
                    height: parent.height
                    radius: 3
                    color: Theme.accent
                    Behavior on width { enabled: Motion.enabled; NumberAnimation { duration: Motion.quick } }
                }
            }
            RowLayout {
                Layout.alignment: Qt.AlignRight
                spacing: 8
                StitchButton {
                    text: root.dev.java.downloading ? qsTr("Hide") : qsTr("Cancel")
                    onClicked: javaDialog.close()
                }
                StitchButton {
                    visible: !root.dev.java.downloading
                    primary: true
                    iconName: "download"
                    text: qsTr("Download Java")
                    onClicked: root.dev.downloadJava()
                }
            }
        }
        Connections {
            target: root.dev
            function onJavaChanged() { if (root.dev.java.found && javaDialog.visible) javaDialog.close(); }
        }
    }

    StitchDialog {
        id: eulaDialog
        property string loader: "fabric"
        title: qsTr("Start a test server?")
        width: 460
        standardButtons: Dialog.NoButton
        closePolicy: Popup.CloseOnEscape

        ColumnLayout {
            width: parent.width
            spacing: 14
            Body {
                textFormat: Text.StyledText
                linkColor: Theme.accent
                onLinkActivated: link => Qt.openUrlExternally(link)
                text: qsTr("A Minecraft server only starts once you agree to the <a href=\"https://aka.ms/MinecraftEULA\">Minecraft EULA</a>. Agreeing here applies to this mod's test servers.")
            }
            Body {
                color: Theme.textDim
                font.pixelSize: 12
                text: qsTr("The test server runs in offline mode, so the development client can join it at localhost.")
            }
            RowLayout {
                Layout.alignment: Qt.AlignRight
                spacing: 8
                StitchButton { text: qsTr("Cancel"); onClicked: eulaDialog.close() }
                StitchButton {
                    primary: true
                    text: qsTr("I agree, start the server")
                    onClicked: { eulaDialog.close(); root.dev.acceptEula(eulaDialog.loader); }
                }
            }
        }
    }
}
