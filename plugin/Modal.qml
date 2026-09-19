import QtQuick
import QtQuick.Layouts
import qs.Commons

// A modal card over the window: a scrim, a title, whatever `content` is.
// Clicking the scrim or pressing Escape (handled by the window) closes it.
Item {
  id: root

  property bool opened: false
  property string title: ""
  // A glyph shown before the title, in the accent colour.
  property string icon: ""
  // A text value the content can bind to, for the one-field dialogs.
  property string text: ""
  property string fontFamily: Style.font.family
  property Component content: null
  property int cardWidth: 460

  visible: opened
  z: 50

  Rectangle {
    anchors.fill: parent
    color: Util.alpha(Color.background, 0.72)
    MouseArea {
      anchors.fill: parent
      onClicked: root.opened = false
    }
  }

  Rectangle {
    id: card
    anchors.centerIn: parent
    width: Math.min(root.cardWidth, root.width - Style.spacing.panelPadding * 2)
    height: column.implicitHeight + Style.spacing.panelPadding * 2
    radius: Style.cornerRadius
    color: Color.popups.background
    border.width: 1
    border.color: Color.popups.border

    MouseArea {
      anchors.fill: parent
      // Swallow clicks so the scrim does not close the card under them.
      onClicked: {}
    }

    ColumnLayout {
      id: column
      anchors.fill: parent
      anchors.margins: Style.spacing.panelPadding
      spacing: Style.spacing.md

      RowLayout {
        Layout.fillWidth: true
        visible: root.title !== ""
        spacing: Style.spacing.sm
        Text {
          visible: root.icon !== ""
          text: root.icon
          color: Color.accent
          font.family: root.fontFamily
          font.pixelSize: Style.font.iconLarge
        }
        Text {
          Layout.fillWidth: true
          text: root.title
          color: Color.popups.text
          font.family: root.fontFamily
          font.pixelSize: Style.font.title
          font.bold: true
          wrapMode: Text.Wrap
        }
      }

      Loader {
        Layout.fillWidth: true
        active: root.opened
        sourceComponent: root.content
      }
    }
  }
}
