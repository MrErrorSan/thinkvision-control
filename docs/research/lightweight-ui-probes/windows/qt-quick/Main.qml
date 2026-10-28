import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ApplicationWindow {
    title: "probe"
    width: 600
    height: 400
    visible: true
    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 24
        spacing: 12
        Label { text: "Brightness" }
        Slider { from: 0; to: 100; value: 50; Layout.fillWidth: true }
        Switch { text: "Auto" }
        ComboBox { model: ["sRGB", "DCI-P3", "Custom"] }
        Button { text: "Apply" }
        Item { Layout.fillHeight: true }
    }
}
