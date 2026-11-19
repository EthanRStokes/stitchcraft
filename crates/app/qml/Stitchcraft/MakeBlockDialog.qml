import QtQuick
import QtQuick.Controls
import com.blockworked.Blockstitch

// "Make a Block": blockstitch's shared definition editor. A mod can't run a
// branch, so the editor doesn't offer one.
StitchDialog {
    id: root
    title: editBlockId.length ? qsTr("Edit block") : qsTr("Make a block")
    width: Math.min(800, parent ? parent.width - 32 : 800)
    height: visible ? Math.min(640, parent ? parent.height - 32 : 640) : 640
    standardButtons: Dialog.NoButton

    // function(command, args, onOk, onError), the window's invoke.
    required property var invoke

    property string editBlockId: ""
    property var pendingDef: null

    function openForCreate() {
        editBlockId = "";
        pendingDef = null;
        open();
    }
    function openForEdit(def) {
        editBlockId = def.id;
        pendingDef = def;
        open();
    }
    onOpened: {
        if (pendingDef) editor.load(pendingDef);
        else editor.reset();
        editor.focusFirst();
    }
    onClosed: {
        editor.closePopups();
        editBlockId = "";
        pendingDef = null;
    }

    contentItem: BlockDefinitionEditor {
        id: editor
        allowBranches: false
        detailsTitle: qsTr("Details")
        detailsIcon: "info"
        detailsNote: qsTr("A name and description for this block, shown in its Details.")
        identifierPlaceholder: qsTr("BlockName")
        onCreateRequested: draft => root.invoke("create_block", draft, () => root.accept())
        onEditRequested: (blockId, draft) => root.invoke("edit_block", Object.assign({ blockId: blockId }, draft), () => root.accept())
        onCancelled: root.reject()
    }
}
