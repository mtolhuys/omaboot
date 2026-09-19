import QtQuick
import QtQuick.Layouts
import qs.Commons
import qs.Ui
import "icons.js" as Icons

// The read-only inspector of what boots now: facts, each with where it was
// read from, then what is wrong, if anything. Nothing here is editable; the
// way to change it is to make a theme from it.
GridLayout {
  id: root

  property var info: null
  property string screen: "unlock"
  property string fontFamily: Style.font.family
  property color muted: Util.alpha(Color.foreground, 0.55)
  columns: 1
  rowSpacing: Style.spacing.sm
  columnSpacing: Style.spacing.panelGap

  RowLayout {
    Layout.columnSpan: root.columns
    spacing: Style.spacing.sm
    Text {
      text: root.screen === "login" ? Icons.login : (root.screen === "shutdown" ? Icons.shutdown : Icons.unlock)
      color: root.muted
      font.family: root.fontFamily
      font.pixelSize: Style.font.iconSmall
    }
    PanelSectionHeader {
      text: root.screen === "login" ? "LOGIN SCREEN · SDDM" : "BOOT AND SHUTDOWN · PLYMOUTH"
      fontFamily: root.fontFamily
    }
    Text {
      text: "·  read just now"
      color: root.muted
      font.family: root.fontFamily
      font.pixelSize: Style.font.caption
    }
    Item { Layout.fillWidth: true }
  }

  Repeater {
    model: root.info && root.info.facts ? root.info.facts[root.screen] : []

    ColumnLayout {
      required property var modelData
      Layout.fillWidth: true
      Layout.maximumWidth: root.columns > 1 ? 300 : -1
      Layout.alignment: Qt.AlignTop | Qt.AlignLeft
      Layout.topMargin: Style.spacing.xs
      spacing: 0

      Text {
        text: modelData.label
        color: root.muted
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
      }
      RowLayout {
        Layout.fillWidth: true
        spacing: Style.spacing.xs
        Rectangle {
          visible: String(modelData.value).match(/^#[0-9a-fA-F]{6}$/) !== null
          width: 14; height: 14
          radius: 3
          color: visible ? modelData.value : "transparent"
          border.width: 1
          border.color: Util.alpha(Color.foreground, 0.3)
        }
        Text {
          Layout.fillWidth: true
          text: modelData.value
          color: Color.foreground
          font.family: root.fontFamily
          font.pixelSize: Style.font.body
          wrapMode: Text.WrapAnywhere
        }
      }
    }
  }

  Text {
    Layout.fillWidth: true
    Layout.columnSpan: root.columns
    Layout.topMargin: Style.spacing.md
    text: Icons.info + "  Nothing here is edited in place. \"Make a theme from this\" copies it into a theme of yours; applying that is what changes the system."
    color: Color.accent
    font.family: root.fontFamily
    font.pixelSize: Style.font.caption
    wrapMode: Text.Wrap
  }
}
