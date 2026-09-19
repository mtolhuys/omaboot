import QtQuick
import QtQuick.Layouts
import qs.Commons
import qs.Ui
import "icons.js" as Icons

// An apply, dry run, revert or reset while it runs and after it finished:
// the steps as they complete, then every operation the engine reported,
// and, if it went wrong, the reason in the engine's own words.
Item {
  id: root

  property bool opened: false
  property string fontFamily: Style.font.family
  property color muted: Util.alpha(Color.foreground, 0.55)

  property string action: ""
  property string theme: ""
  property var steps: []
  property var report: []
  property bool finished: false
  property bool ok: false
  property string message: ""

  signal closed()
  signal passwordRefused()

  visible: opened
  z: 70

  readonly property string title: {
    var names = { dryrun: "Dry run", apply: "Apply", revert: "Revert", reset: "Remove omaboot",
      "login-stock": "Use Omarchy's login screen", "login-release": "Give the login screen back" }
    var what = names[action] || action
    return theme ? what + " · " + theme : what
  }
  readonly property string icon: {
    var icons = { dryrun: Icons.dryrun, apply: Icons.apply, revert: Icons.revert, reset: Icons.reset,
      "login-stock": Icons.login, "login-release": Icons.release }
    return icons[action] || ""
  }

  function reset(nextAction, nextTheme) {
    action = nextAction
    theme = nextTheme
    steps = []
    report = []
    finished = false
    ok = false
    message = ""
  }

  function onLine(line) {
    if (line.event === "step") {
      var next = steps.slice()
      var found = false
      for (var i = 0; i < next.length; i++) {
        if (next[i].step === line.step) { next[i] = { step: line.step, title: line.title, state: line.state }; found = true }
      }
      if (!found) next.push({ step: line.step, title: line.title, state: line.state })
      steps = next
    } else if (line.event === "done") {
      var lines = []
      for (var s = 0; s < line.steps.length; s++) {
        var step = line.steps[s]
        lines.push({ kind: "step", text: step.title + (step.skipped ? " (not selected)" : "") })
        for (var o = 0; o < step.operations.length; o++) lines.push({ kind: "op", text: step.operations[o] })
        for (var p = 0; p < step.problems.length; p++) lines.push({ kind: "problem", text: "! " + step.problems[p] })
      }
      report = lines
    }
  }

  function onDone(isOk, last, errorMessage) {
    finished = true
    ok = isOk
    message = isOk ? "" : errorMessage
    if (!isOk && errorMessage.indexOf("password was not accepted") !== -1) {
      opened = false
      passwordRefused()
    }
  }

  Rectangle {
    anchors.fill: parent
    color: Util.alpha(Color.background, 0.85)
  }

  Rectangle {
    anchors.fill: parent
    anchors.margins: Style.spacing.panelPadding * 2
    radius: Style.cornerRadius
    color: Color.popups.background
    border.width: 1
    border.color: Color.popups.border

    ColumnLayout {
      anchors.fill: parent
      anchors.margins: Style.spacing.panelPadding
      spacing: Style.spacing.md

      RowLayout {
        spacing: Style.spacing.sm
        Text {
          text: root.icon
          color: Color.accent
          font.family: root.fontFamily
          font.pixelSize: Style.font.iconLarge
        }
        Text {
          text: root.title
          color: Color.popups.text
          font.family: root.fontFamily
          font.pixelSize: Style.font.title
          font.bold: true
        }
      }

      Repeater {
        model: root.steps
        RowLayout {
          required property var modelData
          spacing: Style.spacing.sm
          Text {
            text: modelData.state === "finished" ? Icons.done : Icons.working
            color: modelData.state === "finished" ? Color.popups.text : Color.accent
            font.family: root.fontFamily
            font.pixelSize: Style.font.icon
          }
          Text {
            text: modelData.title
            color: modelData.state === "finished" ? Color.popups.text : Color.accent
            font.family: root.fontFamily
            font.pixelSize: Style.font.body
          }
        }
      }

      Text {
        Layout.fillWidth: true
        visible: root.finished
        text: !root.ok ? root.message
          : (root.action === "dryrun" ? "Nothing was changed: that was a dry run. Below is what apply would do, step by step."
          : (root.action === "apply" ? "Done. Reboot to see it. Revert puts everything back; below is what was done."
          : "Done. Below is what was done."))
        color: root.ok ? Color.accent : Color.urgent
        font.family: root.fontFamily
        font.pixelSize: Style.font.body
        wrapMode: Text.Wrap
      }

      Flickable {
        Layout.fillWidth: true
        Layout.fillHeight: true
        clip: true
        contentHeight: reportColumn.implicitHeight
        boundsBehavior: Flickable.StopAtBounds

        ColumnLayout {
          id: reportColumn
          width: parent.width
          spacing: 1
          Repeater {
            model: root.report
            Text {
              required property var modelData
              Layout.fillWidth: true
              Layout.leftMargin: modelData.kind === "step" ? 0 : Style.spacing.xl
              Layout.topMargin: modelData.kind === "step" ? Style.spacing.sm : 0
              text: modelData.kind === "problem" ? Icons.problem + " " + modelData.text.replace(/^! /, "") : modelData.text
              color: modelData.kind === "problem" ? Color.urgent : Color.popups.text
              font.family: root.fontFamily
              font.pixelSize: modelData.kind === "step" ? Style.font.body : Style.font.caption
              font.bold: modelData.kind === "step"
              wrapMode: Text.WrapAnywhere
            }
          }
        }
      }

      RowLayout {
        Layout.fillWidth: true
        Text {
          Layout.fillWidth: true
          text: root.finished ? "" : "working; the steps fill in as they finish"
          color: root.muted
          font.family: root.fontFamily
          font.pixelSize: Style.font.caption
        }
        Button {
          iconText: Icons.back
          text: "Back"
          bordered: true
          enabled: root.finished
          onClicked: { root.opened = false; root.closed() }
        }
      }
    }
  }
}
