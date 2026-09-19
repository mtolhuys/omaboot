import QtQuick
import QtQuick.Window

// The omaboot mark: an open padlock on a 16-cell grid, drawn from the same
// rows as brand/icon.txt, one rectangle per run of cells, in whatever colour
// the theme gives it. Drawn rather than loaded so it is crisp at any size
// and recolours with the theme like every other glyph in the window.
Item {
  id: root

  property color color: "white"
  property real size: 16

  readonly property var rows: [
    "................",
    "....########....",
    "...##########...",
    "..###......###..",
    "..##........##..",
    "..##........##..",
    "..##............",
    "..##............",
    "..##............",
    ".##############.",
    ".##############.",
    ".######..######.",
    ".######..######.",
    ".##############.",
    ".##############.",
    "................"
  ]

  readonly property var runs: {
    var out = []
    for (var y = 0; y < rows.length; y++) {
      var x = 0
      while (x < rows[y].length) {
        if (rows[y][x] === "#") {
          var start = x
          while (x < rows[y].length && rows[y][x] === "#") x++
          out.push({ x: start, y: y, w: x - start })
        } else {
          x++
        }
      }
    }
    return out
  }

  // Every cell is a whole number of device pixels, at least two, so the
  // mark is the same crisp shape on a 1x, 1.25x or 2x display instead of
  // a smear of one- and two-pixel cells. The mark may therefore come out a
  // little larger or smaller than `size` asks for.
  readonly property real dpr: Screen.devicePixelRatio > 0 ? Screen.devicePixelRatio : 1
  readonly property int cellDevice: Math.max(2, Math.round(size * dpr / 16))
  readonly property real cell: cellDevice / dpr

  implicitWidth: cell * 16
  implicitHeight: cell * 16

  Repeater {
    model: root.runs
    Rectangle {
      required property var modelData
      x: modelData.x * root.cell
      y: modelData.y * root.cell
      width: modelData.w * root.cell
      height: root.cell
      color: root.color
      antialiasing: false
    }
  }
}
