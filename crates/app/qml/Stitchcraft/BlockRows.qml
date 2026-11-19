import QtQuick
import com.blockworked.Blockstitch

// Hands the Minecraft block vocabulary to blockstitch's BlockRegistry.
//
// The tables are built in Rust (`stitchcraft_blocks::catalog` and
// `reporters`) so the block enum and its drawing can't drift apart, and
// arrive in one `catalogJson`. Two things JSON can't carry are put back here:
//
//   * a dropdown whose choices depend on the document, written as
//     `options: {source: "declaredBlocks"}` and turned into the function
//     BlockRegistry asks for;
//   * a boolean field, written as `options: "yesno"`: a two-entry dropdown
//     with an encode/decode pair, since the field is a real JSON boolean.
QtObject {
    id: root

    // The Backend, for the catalogue and the current choice sets.
    required property var backend

    readonly property var catalog: JSON.parse(backend.catalogJson || "{}")
    readonly property var choices: JSON.parse(backend.choicesJson || "{}")

    // A newly declared block shows up in every "when [block] is right-clicked"
    // dropdown straight away.
    onChoicesChanged: BlockRegistry.revision++

    readonly property var yesNoOptions: [
        { value: "true", label: qsTr("yes") },
        { value: "false", label: qsTr("no") }
    ]

    function sourceChoices(name) {
        const names = root.choices[name] || [];
        if (!names.length) return [{ value: "", label: qsTr("none yet") }];
        return names.map(n => ({ value: n, label: n }));
    }

    function resolvePiece(piece) {
        if (piece.kind !== "dropdown") return piece;
        const out = Object.assign({}, piece);
        if (out.options === "yesno") {
            out.options = root.yesNoOptions;
            out.encode = chosen => chosen === "true";
            out.decode = value => value ? "true" : "false";
        } else if (out.options && out.options.source !== undefined) {
            const name = out.options.source;
            out.options = () => root.sourceChoices(name);
        }
        return out;
    }

    function register() {
        const rows = root.catalog.rows || {};
        const resolved = {};
        for (const type in rows) {
            const row = Object.assign({}, rows[type]);
            row.head = (row.head || []).map(resolvePiece);
            resolved[type] = row;
        }
        BlockRegistry.registerRows(resolved);
        BlockRegistry.registerOperators(root.catalog.operators || {});
        // What a fresh reporter holds comes from blockstitch-core.
        BlockRegistry.registerDescriptors(root.catalog.descriptors || []);
        // Prefabs arrive without their `type`, which BlockRegistry.prefab adds.
        BlockRegistry.registerPrefabs(root.catalog.prefabs || {});
    }

    Component.onCompleted: register()
}
