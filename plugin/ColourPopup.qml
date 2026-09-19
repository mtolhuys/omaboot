import QtQuick
import QtQuick.Layouts
import qs.Commons
import qs.Ui
import "icons.js" as Icons

// Picking a colour: the palette of the Omarchy theme the desktop is on
// first, since that is what most people want their boot screen to match,
// then the colours of the dropped wallpaper, then every other Omarchy
// theme, then a hex field for anything else.
Item {
  id: root

  property bool opened: false
  property string key: ""
  property string current: ""
  property string fontFamily: Style.font.family
  property var omarchyThemes: []
  property string activeOmarchyTheme: ""
  property var wallpaperPalette: []

  signal picked(string key, string value)

  visible: opened
  z: 60

  function openFor(fieldKey, value) {
    key = fieldKey
    current = value
    hexField.text = value
    opened = true
  }

  function choose(value) {
    opened = false
    picked(key, value)
  }

  readonly property var activePalette: {
    for (var i = 0; i < omarchyThemes.length; i++) {
      if (omarchyThemes[i].name === activeOmarchyTheme && omarchyThemes[i].palette) return omarchyThemes[i].palette
    }
    return null
  }

  function paletteSwatches(palette) {
    if (!palette) return []
    return [
      { label: "background", value: palette.background },
      { label: "text", value: palette.foreground },
      { label: "accent", value: palette.accent },
      { label: "error", value: palette.error }
    ]
  }

  Rectangle {
    anchors.fill: parent
    color: Util.alpha(Color.background, 0.72)
    MouseArea { anchors.fill: parent; onClicked: root.opened = false }
  }

  Rectangle {
    anchors.centerIn: parent
    width: Math.min(560, root.width - Style.spacing.panelPadding * 2)
    height: Math.min(column.implicitHeight + Style.spacing.panelPadding * 2, root.height - Style.spacing.panelPadding * 2)
    radius: Style.cornerRadius
    color: Color.popups.background
    border.width: 1
    border.color: Color.popups.border
    clip: true

    MouseArea { anchors.fill: parent; onClicked: {} }

    Flickable {
      anchors.fill: parent
      anchors.margins: Style.spacing.panelPadding
      contentHeight: column.implicitHeight
      clip: true

      ColumnLayout {
        id: column
        width: parent.width
        spacing: Style.spacing.md

        RowLayout {
          spacing: Style.spacing.sm
          Text {
            text: Icons.colour
            color: Color.accent
            font.family: root.fontFamily
            font.pixelSize: Style.font.icon
          }
          Text {
            text: root.key
            color: Color.popups.text
            font.family: root.fontFamily
            font.pixelSize: Style.font.title
            font.bold: true
          }
        }

        // The active Omarchy theme.
        Text {
          visible: root.activePalette !== null
          text: "Your Omarchy theme" + (root.activeOmarchyTheme ? " (" + root.activeOmarchyTheme + ")" : "")
          color: Util.alpha(Color.popups.text, 0.6)
          font.family: root.fontFamily
          font.pixelSize: Style.font.caption
          font.bold: true
        }
        Flow {
          Layout.fillWidth: true
          visible: root.activePalette !== null
          spacing: Style.spacing.sm
          Repeater {
            model: root.paletteSwatches(root.activePalette)
            Swatch { required property var modelData; value: modelData.value; label: modelData.label; fontFamily: root.fontFamily; onChosen: v => root.choose(v) }
          }
        }

        // The wallpaper.
        Text {
          visible: root.wallpaperPalette.length > 0
          text: "From the wallpaper you dropped"
          color: Util.alpha(Color.popups.text, 0.6)
          font.family: root.fontFamily
          font.pixelSize: Style.font.caption
          font.bold: true
        }
        Flow {
          Layout.fillWidth: true
          visible: root.wallpaperPalette.length > 0
          spacing: Style.spacing.sm
          Repeater {
            model: root.wallpaperPalette
            Swatch { required property var modelData; value: modelData; label: ""; fontFamily: root.fontFamily; onChosen: v => root.choose(v) }
          }
        }

        // Every other Omarchy theme.
        Text {
          text: "Other Omarchy themes"
          color: Util.alpha(Color.popups.text, 0.6)
          font.family: root.fontFamily
          font.pixelSize: Style.font.caption
          font.bold: true
        }
        Repeater {
          model: root.omarchyThemes
          RowLayout {
            required property var modelData
            Layout.fillWidth: true
            visible: modelData.palette && modelData.name !== root.activeOmarchyTheme
            spacing: Style.spacing.sm
            Text {
              Layout.preferredWidth: 130
              text: modelData.name
              color: Color.popups.text
              font.family: root.fontFamily
              font.pixelSize: Style.font.caption
              elide: Text.ElideRight
            }
            Repeater {
              model: root.paletteSwatches(modelData.palette)
              Swatch { required property var modelData; value: modelData.value; label: ""; small: true; fontFamily: root.fontFamily; onChosen: v => root.choose(v) }
            }
          }
        }

        // Anything else.
        Text {
          text: "Or type one"
          color: Util.alpha(Color.popups.text, 0.6)
          font.family: root.fontFamily
          font.pixelSize: Style.font.caption
          font.bold: true
        }
        RowLayout {
          Layout.fillWidth: true
          spacing: Style.spacing.sm
          Rectangle {
            width: Style.spacing.controlHeight; height: width
            radius: 4
            color: /^#[0-9a-fA-F]{6}$/.test(hexField.text) ? hexField.text : "transparent"
            border.width: 1
            border.color: Util.alpha(Color.popups.text, 0.35)
          }
          TextField {
            id: hexField
            Layout.fillWidth: true
            placeholderText: "#rrggbb"
            onAccepted: if (/^#[0-9a-fA-F]{6}$/.test(text)) root.choose(text)
          }
          Button {
            text: "Use"
            bordered: true
            enabled: /^#[0-9a-fA-F]{6}$/.test(hexField.text)
            onClicked: root.choose(hexField.text)
          }
          Button { text: "Cancel"; bordered: true; onClicked: root.opened = false }
        }
      }
    }
  }

  component Swatch: Rectangle {
    property string value: "#000000"
    property string label: ""
    property bool small: false
    property string fontFamily: Style.font.family
    signal chosen(string value)
    width: small ? 22 : (label ? 96 : 40)
    height: small ? 22 : 40
    radius: 4
    color: value
    border.width: 1
    border.color: Util.alpha(Color.popups.text, 0.35)
    Text {
      anchors.centerIn: parent
      visible: parent.label !== ""
      text: parent.label + "\n" + parent.value
      color: (parseInt(parent.value.substring(1, 3), 16) * 0.2126 + parseInt(parent.value.substring(3, 5), 16) * 0.7152 + parseInt(parent.value.substring(5, 7), 16) * 0.0722) > 128 ? "#000000" : "#ffffff"
      font.family: parent.fontFamily
      font.pixelSize: Style.font.caption
      horizontalAlignment: Text.AlignHCenter
    }
    MouseArea {
      anchors.fill: parent
      cursorShape: Qt.PointingHandCursor
      onClicked: parent.chosen(parent.value)
    }
  }
}
