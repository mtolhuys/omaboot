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
      // A sentence (the override, say) is not a column's worth of text: in
      // the stacked layout it takes the whole row and wraps wide, instead of
      // standing as a tall narrow column that pushes the footer off the
      // window. Values that fit a column keep their column.
      readonly property bool prose: String(modelData.value).length > 60 && String(modelData.value).indexOf("/") !== 0
      Layout.fillWidth: true
      Layout.columnSpan: prose ? root.columns : 1
      Layout.maximumWidth: root.columns > 1 && !prose ? 300 : -1
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
          id: value
          Layout.fillWidth: true
          // A path keeps its shape: the start and the file name stay, the
          // middle goes, and the whole of it is in the tooltip. Anything else
          // wraps at word boundaries.
          readonly property bool isPath: String(modelData.value).indexOf("/") === 0
          text: modelData.value
          color: Color.foreground
          font.family: root.fontFamily
          font.pixelSize: Style.font.body
          wrapMode: isPath ? Text.NoWrap : Text.Wrap
          elide: isPath ? Text.ElideMiddle : Text.ElideNone
          MouseArea {
            id: valueHover
            anchors.fill: parent
            hoverEnabled: value.isPath && value.truncated
            acceptedButtons: Qt.NoButton
          }
          PanelToolTip {
            visible: value.isPath && value.truncated && valueHover.containsMouse
            text: modelData.value
          }
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
