import QtQuick
import com.blockworked.Blockstitch
import "qrc:/stitchcraft/HostIcons.js" as HostIcons

// The entry point. It adds Stitchcraft's icons to blockstitch's set, and
// only then creates the window: an icon that draws before its path exists
// keeps the fallback glyph for good.
QtObject {
    id: start
    property var window: null
    Component.onCompleted: {
        Icons.add(HostIcons.paths);
        const component = Qt.createComponent("com.blockworked.Stitchcraft", "Main");
        if (component.status === Component.Error) {
            console.error(component.errorString());
            Qt.exit(1);
            return;
        }
        start.window = component.createObject(null);
    }
}
