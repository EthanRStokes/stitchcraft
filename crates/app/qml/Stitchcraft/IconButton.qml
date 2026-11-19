import QtQuick
import QtQuick.Controls
import com.blockworked.Blockstitch

// A square toolbar button: one Lucide icon, its action in a tooltip.
StitchButton {
    property string tip: ""
    text: ""
    implicitWidth: 34
    implicitHeight: 34
    ToolTip.visible: hovered && tip.length > 0
    ToolTip.delay: 500
    ToolTip.text: tip
}
