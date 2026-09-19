import QtQuick
import QtQuick.Window

// Quickshell's FloatingWindow, as far as the plugin uses it: a titled
// window with a colour, an implicit size and a minimum size.
Window {
  id: root

  property real implicitWidth: 640
  property real implicitHeight: 480
  property size minimumSize: Qt.size(0, 0)

  width: implicitWidth
  height: implicitHeight
  minimumWidth: minimumSize.width
  minimumHeight: minimumSize.height
}
