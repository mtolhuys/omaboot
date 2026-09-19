import QtQuick
import QtQuick.Layouts
import qs.Commons
import qs.Ui
import "icons.js" as Icons

// The editable properties of one theme, for one screen. Each row is one
// field the engine described (`omaboot show --json`): its kind decides the
// control, its value comes from the engine, and every change goes back
// through `edited(key, value)` where the engine validates it and saves.
//
// Fields come in groups (Colours, Logo, Unlock, ...). In a column the groups
// follow each other; along the bottom of the window each group is a column
// of its own, so a group never breaks across two.
ColumnLayout {
  id: root

  property var theme: null
  property string screen: "unlock"
  property string fontFamily: Style.font.family
  property color muted: Util.alpha(Color.foreground, 0.55)
  // How many groups sit beside each other: one in a column, more along the
  // bottom of the window, where the groups wrap into rows.
  property int columns: 1
  spacing: Style.spacing.md

  signal edited(string key, string value)
  signal pickColour(string key, string current)
  // Open the file dialog for an image role: logo, shutdown-logo, background.
  signal chooseFile(string role)

  // A field loses focus far more often than it changes: "0.50" typed over
  // "0.5", or "#D35F5F" over "#d35f5f", is the same value and is not saved.
  function same(kind, typed, stored) {
    if (kind === "number") return Number(typed) === Number(stored)
    if (kind === "colour") return String(typed).trim().toLowerCase().replace(/^#?/, "#") === String(stored).toLowerCase()
    return String(typed) === String(stored)
  }

  readonly property var groups: {
    if (!theme || !theme.fields) return []
    var out = []
    var byName = {}
    for (var i = 0; i < theme.fields.length; i++) {
      var field = theme.fields[i]
      if (field.screens.indexOf(screen) === -1) continue
      if (!byName[field.group]) {
        byName[field.group] = { name: field.group, fields: [] }
        out.push(byName[field.group])
      }
      byName[field.group].fields.push(field)
    }
    return out
  }

  Text {
    Layout.fillWidth: true
    visible: root.theme && root.theme.problem
    text: root.theme && root.theme.problem ? Icons.problem + "  " + root.theme.problem : ""
    color: Color.urgent
    font.family: root.fontFamily
    font.pixelSize: Style.font.caption
    wrapMode: Text.Wrap
  }

  GridLayout {
    Layout.fillWidth: true
    columns: Math.max(1, root.columns)
    rowSpacing: Style.spacing.lg
    columnSpacing: Style.spacing.panelGap * 2

    Repeater {
      model: root.groups

      ColumnLayout {
        id: group
        required property var modelData
        Layout.fillWidth: true
        Layout.maximumWidth: root.columns > 1 ? 420 : -1
        Layout.alignment: Qt.AlignTop | Qt.AlignLeft
        spacing: Style.spacing.sm

        PanelSectionHeader {
          text: String(group.modelData.name).toUpperCase()
          fontFamily: root.fontFamily
        }

        Repeater {
          model: group.modelData.fields

          RowLayout {
            id: row
            required property var modelData
            Layout.fillWidth: true
            spacing: Style.spacing.sm

            readonly property string kind: modelData.kind.type
            // The logo width (a share of the screen width) gets a slider;
            // every other number is typed.
            readonly property bool isScale: /\.width$/.test(modelData.key)

            Text {
              Layout.preferredWidth: 104
              text: row.modelData.label
              color: Color.foreground
              font.family: root.fontFamily
              font.pixelSize: Style.font.body
              elide: Text.ElideRight
            }

            // Colour: a swatch that opens the palette, and the hex to type.
            Rectangle {
              visible: row.kind === "colour"
              width: Style.spacing.controlHeight - 4
              height: width
              radius: 4
              color: row.kind === "colour" && /^#[0-9a-fA-F]{6}$/.test(row.modelData.value) ? row.modelData.value : "transparent"
              border.width: 1
              border.color: Util.alpha(Color.foreground, 0.35)
              MouseArea {
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                onClicked: root.pickColour(row.modelData.key, row.modelData.value)
              }
            }
            TextField {
              visible: row.kind === "colour" || row.kind === "text" || (row.kind === "number" && !row.isScale)
              Layout.fillWidth: true
              text: row.modelData.value
              placeholderText: row.kind === "colour" ? "#rrggbb" : (row.kind === "text" ? "leave empty for none" : "")
              onEditingFinished: if (!root.same(row.kind, text, row.modelData.value)) root.edited(row.modelData.key, text)
            }

            // Choice: a dropdown; the shutdown logo also offers "inherit".
            Dropdown {
              visible: row.kind === "choice" || row.kind === "file"
              Layout.fillWidth: true
              Layout.preferredWidth: 100
              showLabel: false
              value: row.modelData.value
              options: {
                if (row.kind === "choice") return row.modelData.kind.options
                var images = root.theme ? root.theme.images.slice() : []
                if (row.modelData.key === "shutdown.logo") images.unshift("inherit")
                if (images.indexOf(row.modelData.value) === -1) images.unshift(row.modelData.value)
                return images
              }
              onChanged: value => { if (value !== row.modelData.value) root.edited(row.modelData.key, value) }
            }

            // Pick an image from disk for the logo, the shutdown logo, or the
            // login wallpaper; dropping one on the picture does the same.
            PanelActionButton {
              visible: row.kind === "file" || row.modelData.key === "login.background"
              iconText: row.modelData.key === "login.background" ? Icons.wallpaper : Icons.image
              bordered: true
              tooltipText: row.modelData.key === "login.background" ? "Pick a wallpaper (PNG or JPG), or drop one on the picture"
                : (row.modelData.key === "shutdown.logo" ? "Pick the shutdown logo (PNG or SVG), or drop one on the picture" : "Pick the logo (PNG or SVG), or drop one on the picture")
              onClicked: root.chooseFile(row.modelData.key === "login.background" ? "background"
                : (row.modelData.key === "shutdown.logo" ? "shutdown-logo" : "logo"))
            }

            // Toggle.
            // The engine says "on" and "off" for a toggle.
            ToggleSwitch {
              visible: row.kind === "toggle"
              checked: row.modelData.value === "on"
              onToggled: root.edited(row.modelData.key, row.modelData.value === "on" ? "off" : "on")
            }
            Item { visible: row.kind === "toggle"; Layout.fillWidth: true }

            // The logo width: a slider, with the number beside it.
            PanelSlider {
              visible: row.kind === "number" && row.isScale
              Layout.fillWidth: true
              minimum: row.modelData.kind.min !== undefined ? row.modelData.kind.min : 0
              maximum: row.modelData.kind.max !== undefined ? row.modelData.kind.max : 1
              step: row.modelData.kind.step !== undefined ? row.modelData.kind.step : 0.01
              value: Number(row.modelData.value) || 0.42
              onReleased: v => root.edited(row.modelData.key, (Math.round(v * 100) / 100).toFixed(2))
            }
            TextField {
              visible: row.kind === "number" && row.isScale
              Layout.preferredWidth: 64
              text: Number(row.modelData.value).toFixed(2)
              onEditingFinished: if (!root.same("number", text, row.modelData.value)) root.edited(row.modelData.key, text)
            }
          }
        }
      }
    }
  }
}
