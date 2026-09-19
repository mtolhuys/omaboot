import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui
import "icons.js" as Icons

// omaboot: the Plymouth unlock, SDDM login and Plymouth shutdown screens,
// looked at and changed in one window.
//
// The window opens on what boots now, read from the system through the
// engine, never cached. Your themes sit under it. A theme is changed in the
// properties column, the picture follows every change, and one button
// applies it. Nothing here touches a file or the system itself: every
// change is a call into `omaboot --json` (see Engine.qml), so the plugin
// is a view over the same rules the command line uses.
Item {
  id: root

  // Injected by omarchy-shell.
  property string omarchyPath: Quickshell.env("OMARCHY_PATH")
  property var shell: null
  property var manifest: null
  readonly property string pluginId: "mtolhuys.omaboot"

  // ---- lifecycle ---------------------------------------------------------

  property bool opened: false
  property bool closingFromHost: false

  function open(payloadJson) {
    closingFromHost = false
    opened = true
    window.visible = true
    refresh()
  }

  function close() {
    closingFromHost = true
    window.visible = false
    opened = false
    closingFromHost = false
  }

  function requestClose() {
    opened = false
    window.visible = false
    if (shell && typeof shell.hide === "function") shell.hide(pluginId)
  }

  // ---- state -------------------------------------------------------------

  // What `omaboot status --json` said last.
  property var info: null
  // "now" for the system entry, or the id of one of your themes.
  property string selection: "now"
  // What `omaboot show <id> --json` said for the selected theme.
  property var theme: null
  property string screen: "unlock"
  // Bumped whenever the picture would change, so a preview is drawn exactly
  // once per change and never confused with an older one.
  property int revision: 0
  // screen -> { source, message } for the current selection and revision.
  property var previews: ({})
  property string status: ""
  property bool statusIsError: false
  property string busy: ""
  // The password lives here only between the dialog and the engine call.
  property string password: ""
  // The colours of the last dropped or chosen wallpaper, offered as swatches.
  property var wallpaperPalette: []

  // One layout: a sidebar with the system and your themes, the picture in
  // the middle, the inspector on the right. Below `narrowBelow` there is no
  // room for three columns, and the same three blocks stack: the sidebar
  // becomes a strip of cards, the inspector goes under the picture.
  readonly property int narrowBelow: 1180
  readonly property bool stacked: window.width < narrowBelow
  readonly property int sidebarWidth: 280
  readonly property int inspectorWidth: 400
  // The strip of cards along the top of the stacked layout.
  readonly property int stripHeight: 96
  readonly property int cardWidth: 380

  readonly property bool onSystem: selection === "now"
  readonly property var screens: ["unlock", "login", "shutdown"]
  readonly property string fontFamily: Style.font.family
  readonly property color muted: Util.alpha(Color.foreground, 0.55)
  readonly property color faint: Util.alpha(Color.foreground, 0.12)
  readonly property color panel: Util.alpha(Color.foreground, 0.04)

  function say(text, isError) {
    status = text || ""
    statusIsError = isError === true
  }

  function refresh(keepSelection) {
    busy = "reading the system"
    engine.inspect(function(ok, data, message) {
      busy = ""
      if (!ok) { say(message, true); return }
      info = data
      if (keepSelection !== false && selection !== "now") {
        var stillThere = false
        for (var i = 0; i < data.themes.length; i++) if (data.themes[i].id === selection) stillThere = true
        if (!stillThere) selection = "now"
      }
      if (selection === "now") {
        theme = null
        revision += 1
        if (status === "") say("showing what boots now: " + data.headline)
      } else {
        loadTheme(selection)
      }
    })
  }

  function select(id) {
    if (selection === id) return
    selection = id
    previews = ({})
    say("")
    if (id === "now") {
      theme = null
      revision += 1
    } else {
      loadTheme(id)
    }
  }

  function loadTheme(id) {
    engine.show(id, function(ok, data, message) {
      if (!ok) { say(message, true); return }
      if (selection !== id) return
      theme = data
      revision += 1
    })
  }

  // The engine writes the picture; the window shows the file. A stale
  // answer (older revision, other selection) is dropped.
  function draw() {
    if (!opened) return
    var sel = selection, scr = screen, rev = revision
    if (sel === "now" && info && info.pictures && info.pictures[scr] === false) {
      var copy = Object.assign({}, previews)
      copy[scr] = { source: "", message: noPictureMessage(scr) }
      previews = copy
      return
    }
    if (sel !== "now" && theme && theme.valid === false) {
      var copy2 = Object.assign({}, previews)
      copy2[scr] = { source: "", message: "This theme cannot be drawn yet: " + theme.problem }
      previews = copy2
      return
    }
    var out = engine.runtimeDir + "/" + (sel === "now" ? "now" : "theme-" + sel) + "-" + scr + "-" + rev + ".png"
    engine.render(sel === "now" ? "" : sel, scr, out, "960x540", function(ok, data, message) {
      if (sel !== selection || rev !== revision) return
      var next = Object.assign({}, previews)
      next[scr] = ok ? { source: "file://" + out, message: "" } : { source: "", message: message }
      previews = next
    })
  }

  function noPictureMessage(scr) {
    if (!info) return ""
    if (scr === "login") {
      var t = info.login.theme
      if (!t) return "No SDDM configuration names a login theme."
      return "Your login screen is the SDDM theme " + t + ", chosen by " + info.login.decided_by
        + ".\nomaboot does not know its layout, so it draws no picture of it; \"Show for real\" runs the real greeter."
        + "\nApplying a theme of yours replaces it with omaboot's login screen; Revert brings it back."
    }
    if (!info.plymouth.theme) return info.plymouth.conf + " names no Plymouth theme."
    if (!info.plymouth.dir) return "The Plymouth theme " + info.plymouth.theme + " is not installed."
    return "The installed theme does not have the files omaboot knows how to draw. Show it for real instead."
  }

  onRevisionChanged: draw()
  onScreenChanged: { if (!previews[screen]) draw() }

  // ---- editing -----------------------------------------------------------

  // Edits are saved one at a time. The engine reads theme.toml, changes it
  // and writes it back, so two saves in flight at once would race and the
  // later one could bring back what the earlier one changed. While a save
  // runs, further edits wait here and go out together when it is done.
  property var pendingEdits: ({})
  property bool saving: false
  // Other writes to the theme (an image, a rename) wait their turn the same way.
  property var afterSave: []

  function whenIdle(fn) {
    if (saving) { var q = afterSave.slice(); q.push(fn); afterSave = q } else fn()
  }

  function saveDone() {
    saving = false
    if (Object.keys(pendingEdits).length > 0) { flushEdits(); return }
    if (afterSave.length > 0) {
      var q = afterSave.slice()
      var fn = q.shift()
      afterSave = q
      fn()
    }
  }

  function setField(key, value) {
    if (!theme) return
    var next = Object.assign({}, pendingEdits)
    next[key] = value
    pendingEdits = next
    flushEdits()
  }

  function flushEdits() {
    if (saving || !theme) return
    var keys = Object.keys(pendingEdits)
    if (keys.length === 0) return
    var id = theme.id
    var assignments = keys.map(function(k) { return k + "=" + pendingEdits[k] })
    pendingEdits = ({})
    saving = true
    busy = "saving"
    engine.set(id, assignments, function(ok, data, message) {
      busy = ""
      if (!ok) say(message, true)
      else if (selection === id) {
        theme = data
        revision += 1
        say("")
      }
      saveDone()
    })
  }

  function fieldValue(key) {
    if (!theme) return ""
    for (var i = 0; i < theme.fields.length; i++) if (theme.fields[i].key === key) return theme.fields[i].value
    return ""
  }

  function dropFile(path, role) {
    if (onSystem) {
      say("what boots now is not edited in place: make a theme from it first, then drop the image on that", true)
      return
    }
    var id = theme ? theme.id : selection
    whenIdle(function() {
      saving = true
      busy = "adding " + path.split("/").pop()
      engine.addImage(id, path, role, function(ok, data, message) {
        busy = ""
        if (!ok) say(message, true)
        else if (selection === id) {
          theme = data
          revision += 1
          say("added " + path.split("/").pop() + " as the " + (role === "background" ? "login wallpaper" : role.replace("-", " ")))
          if (role === "background") {
            engine.palette(path, function(okP, pal) {
              if (okP) wallpaperPalette = pal.colours
            })
          }
        }
        saveDone()
      })
    })
  }

  // The file dialog runs in its own process (zenity, which Omarchy ships).
  // A Qt dialog inside the shell process blocks and can take the whole shell
  // down with it, which is exactly what the first version did.
  // The dialog opens in ~/Pictures the first time, then wherever the last
  // image was picked from. A directory that does not exist is harmless:
  // zenity falls back to the home directory.
  property string chooserDir: (Quickshell.env("HOME") || "") + "/Pictures/"

  function chooseImage(role) {
    if (chooser.running) return
    chooser.role = role
    chooser.chosen = ""
    chooser.command = ["zenity", "--file-selection",
      "--title=" + (role === "background" ? "Choose a wallpaper for the login screen"
        : (role === "shutdown-logo" ? "Choose the shutdown logo" : "Choose the logo")),
      "--filename=" + chooserDir,
      "--file-filter=Images | *.png *.PNG *.svg *.SVG *.jpg *.JPG *.jpeg *.JPEG *.webp *.WEBP",
      "--file-filter=All files | *"]
    chooser.running = true
    say("the file dialog is open in its own window")
  }

  Process {
    id: chooser
    property string role: "logo"
    property string chosen: ""
    stdout: SplitParser {
      splitMarker: "\n"
      onRead: data => { var line = String(data).trim(); if (line) chooser.chosen = line }
    }
    onExited: (exitCode, exitStatus) => {
      if (exitCode === 0 && chooser.chosen) {
        root.chooserDir = chooser.chosen.substring(0, chooser.chosen.lastIndexOf("/") + 1)
        root.dropFile(chooser.chosen, chooser.role)
      }
      else if (exitCode === 1 || exitCode === 5) root.say("")
      else root.say("no file dialog could be opened (zenity exited with " + exitCode + "); drop the image on the picture instead", true)
      chooser.chosen = ""
    }
  }

  // A drop on the Login tab is a wallpaper; anywhere else it is the logo,
  // or the shutdown logo when that tab is showing.
  function roleForDrop(scr, path) {
    if (scr === "login") return "background"
    if (scr === "shutdown") return "shutdown-logo"
    return "logo"
  }

  function makeTheme(name, source) {
    busy = "creating " + name
    engine.newTheme(name, source, function(ok, data, message) {
      busy = ""
      if (!ok) { say(message, true); newDialog.problem = message; return }
      newDialog.opened = false
      refresh(false)
      selection = data.id
      theme = data
      previews = ({})
      revision += 1
      say("created " + data.id + (source === "current" ? " from what boots now; change it, then apply it" : ""))
    })
  }

  function removeTheme(id) {
    engine.deleteTheme(id, function(ok, data, message) {
      if (!ok) { say(message, true); return }
      selection = "now"
      theme = null
      previews = ({})
      refresh(false)
      say("removed " + id + "; " + data.note)
    })
  }

  // ---- privileged actions ------------------------------------------------

  // "apply" | "dryrun" | "revert" | "reset" | "preview"
  property string pendingAction: ""

  function needsPassword(action) {
    if (info && info.sandbox) return false
    if (action === "dryrun") return false
    if (action === "preview" && screen === "login") return false
    return true
  }

  function start(action) {
    pendingAction = action
    if (needsPassword(action)) {
      passwordDialog.problem = ""
      passwordDialog.opened = true
    } else {
      go(action, "")
    }
  }

  function go(action, pw) {
    var id = onSystem ? "" : selection
    runView.reset(action, id)
    if (action === "apply" || action === "dryrun") {
      runView.opened = true
      engine.apply(id, action === "dryrun", pw, runView.onLine, runView.onDone)
    } else if (action === "revert") {
      runView.opened = true
      engine.revert(pw, runView.onLine, runView.onDone)
    } else if (action === "reset") {
      runView.opened = true
      engine.reset(pw, runView.onLine, runView.onDone)
    } else if (action === "login-stock" || action === "login-release") {
      runView.opened = true
      engine.login(action === "login-stock" ? "stock" : "release", pw, runView.onLine, runView.onDone)
    } else if (action === "preview") {
      say("showing the real " + screen + " screen in a window; close it, or wait, to come back")
      busy = "real " + screen + " screen"
      engine.preview(id, screen, pw, function(line) {
        if (line.event === "say") say(line.message)
      }, function(ok, data, message) {
        busy = ""
        say(ok ? "the real " + screen + " screen closed" : message, !ok)
      })
    }
    pw = ""
    password = ""
  }

  Engine {
    id: engine
    onFailed: message => { if (root.status === "" || !root.statusIsError) root.say(message, true) }
  }

  // ---- the window --------------------------------------------------------

  FloatingWindow {
    id: window
    title: "omaboot"
    color: Color.background
    implicitWidth: 1240
    implicitHeight: 800
    minimumSize: Qt.size(960, 620)

    onVisibleChanged: {
      if (!visible && !root.closingFromHost && root.opened) root.requestClose()
    }

    FocusScope {
      anchors.fill: parent
      focus: true

      Keys.onPressed: function(event) {
        if (event.key === Qt.Key_Escape) {
          if (colourPopup.opened) { colourPopup.opened = false; event.accepted = true; return }
          if (runView.opened && runView.finished) { runView.opened = false; event.accepted = true; return }
          if (newDialog.opened) { newDialog.opened = false; event.accepted = true; return }
          if (confirm.opened) { confirm.opened = false; event.accepted = true; return }
          if (passwordDialog.opened) { passwordDialog.opened = false; event.accepted = true; return }
          root.requestClose()
          event.accepted = true
        } else if (event.key === Qt.Key_1) { root.screen = "unlock"; event.accepted = true }
        else if (event.key === Qt.Key_2) { root.screen = "login"; event.accepted = true }
        else if (event.key === Qt.Key_3) { root.screen = "shutdown"; event.accepted = true }
      }

      ColumnLayout {
        anchors.fill: parent
        anchors.margins: Style.spacing.panelPadding
        spacing: Style.spacing.panelGap

        // Header: name, what boots now, what the engine is doing.
        RowLayout {
          Layout.fillWidth: true
          spacing: Style.spacing.lg

          RowLayout {
            spacing: Style.spacing.sm
            Mark {
              color: Color.accent
              size: Style.font.heading + 8
              Layout.alignment: Qt.AlignVCenter
            }
            Text {
              text: "omaboot"
              color: Color.accent
              font.family: root.fontFamily
              font.pixelSize: Style.font.heading
              font.bold: true
            }
          }
          Text {
            Layout.fillWidth: true
            text: root.info ? root.info.headline : (engine.missing ? "the omaboot engine is not installed" : "reading the system…")
            color: root.muted
            font.family: root.fontFamily
            font.pixelSize: Style.font.body
            elide: Text.ElideRight
          }
          Text {
            visible: root.info && root.info.sandbox
            text: root.info && root.info.sandbox ? "sandbox " + root.info.sandbox : ""
            color: Color.accent
            font.family: root.fontFamily
            font.pixelSize: Style.font.caption
            font.bold: true
          }
          Text {
            visible: root.busy !== ""
            text: Icons.working + "  " + root.busy + "…"
            color: root.muted
            font.family: root.fontFamily
            font.pixelSize: Style.font.caption
          }
          PanelActionButton {
            iconText: Icons.reload
            tooltipText: "Read the system and your themes again"
            onClicked: root.refresh()
          }
          PanelActionButton {
            iconText: Icons.close
            tooltipText: "Close"
            onClicked: root.requestClose()
          }
        }

        // The three blocks. Stacked: a strip of cards, the picture, the
        // properties as group columns. Columns: the same three side by side.
        // One layout, two flows, so nothing is built twice.
        GridLayout {
          Layout.fillWidth: true
          Layout.fillHeight: true
          rowSpacing: Style.spacing.panelGap
          columnSpacing: Style.spacing.panelGap
          flow: root.stacked ? GridLayout.TopToBottom : GridLayout.LeftToRight
          rows: root.stacked ? 3 : 1
          columns: root.stacked ? 1 : 3

          // ---- what boots now, your themes ------------------------------
          // One list in both layouts: the system card is its header, the
          // new-theme card its footer, so a column and a strip are the same
          // thing turned on its side.
          ColumnLayout {
            Layout.preferredWidth: root.stacked ? -1 : root.sidebarWidth
            Layout.minimumWidth: root.stacked ? -1 : root.sidebarWidth
            Layout.fillWidth: root.stacked
            Layout.fillHeight: !root.stacked
            Layout.preferredHeight: root.stacked ? root.stripHeight : -1
            spacing: Style.spacing.md

            ListView {
              id: themeList
              Layout.fillWidth: true
              Layout.fillHeight: true
              clip: true
              orientation: root.stacked ? ListView.Horizontal : ListView.Vertical
              spacing: Style.spacing.sm
              boundsBehavior: Flickable.StopAtBounds
              model: root.info ? root.info.themes : []

              header: Item {
                width: root.stacked ? root.cardWidth + Style.spacing.lg : themeList.width
                height: root.stacked ? themeList.height : systemColumn.implicitHeight + Style.spacing.lg

                ColumnLayout {
                  id: systemColumn
                  anchors.left: parent.left
                  anchors.top: parent.top
                  width: root.stacked ? root.cardWidth : parent.width
                  spacing: Style.spacing.md

                  PanelSectionHeader { text: "SYSTEM"; visible: !root.stacked }

                  Rectangle {
                    id: systemCard
                    Layout.fillWidth: true
                    Layout.preferredWidth: root.stacked ? root.cardWidth : -1
                    Layout.preferredHeight: root.stacked ? root.stripHeight : nowColumn.implicitHeight + Style.spacing.lg * 2
                    radius: Style.cornerRadius
                    color: root.onSystem ? Style.selectedFillFor(Color.foreground, Color.accent) : root.panel
                    border.width: 1
                    border.color: root.onSystem ? Color.accent : (nowMouse.containsMouse ? Util.alpha(Color.foreground, 0.3) : root.faint)

                    MouseArea {
                      id: nowMouse
                      anchors.fill: parent
                      hoverEnabled: true
                      cursorShape: Qt.PointingHandCursor
                      onClicked: root.select("now")
                    }

                    ColumnLayout {
                      id: nowColumn
                      anchors.fill: parent
                      anchors.margins: Style.spacing.lg
                      spacing: Style.spacing.xs

                      RowLayout {
                        Layout.fillWidth: true
                        spacing: Style.spacing.sm
                        Text {
                          text: Icons.system
                          color: root.onSystem ? Color.accent : Color.foreground
                          font.family: root.fontFamily
                          font.pixelSize: Style.font.icon
                        }
                        Text {
                          Layout.fillWidth: true
                          text: "What boots now"
                          color: root.onSystem ? Color.accent : Color.foreground
                          font.family: root.fontFamily
                          font.pixelSize: Style.font.subtitle
                          font.bold: true
                          elide: Text.ElideRight
                        }
                        PanelActionButton {
                          iconText: Icons.copy
                          bordered: true
                          enabled: root.info && root.info.can_copy_current
                          tooltipText: "Make a theme from this: copies the installed colours and logo into a theme of yours"
                          onClicked: { newDialog.source = "current"; newDialog.openWith("") }
                        }
                      }
                      FactLine {
                        icon: Icons.unlock
                        text: root.info ? (root.info.plymouth.theme || "none set")
                          + (root.info.plymouth.styled_by ? "  ·  " + root.info.plymouth.styled_by : "") : ""
                        tip: "Plymouth: the unlock and shutdown screens"
                      }
                      FactLine {
                        icon: Icons.login
                        text: root.info ? (root.info.login.theme || "none set") : ""
                        tip: "SDDM: the login screen"
                      }
                      FactLine {
                        visible: root.info && root.info.applied && root.info.applied.current
                        icon: Icons.boots
                        text: root.info && root.info.applied ? root.info.applied.theme + ", applied " + root.info.applied.age : ""
                        colour: Color.accent
                        tip: "A theme of yours is what boots now"
                      }
                      Item { Layout.fillHeight: true }
                    }
                  }

                  PanelSectionHeader {
                    Layout.topMargin: Style.spacing.sm
                    text: "YOUR THEMES"
                    visible: !root.stacked
                  }
                }
              }

              delegate: Rectangle {
                id: card
                required property var modelData
                readonly property bool selected: root.selection === modelData.id
                width: root.stacked ? 220 : themeList.width
                height: root.stacked ? root.stripHeight : Style.spacing.controlHeight + Style.spacing.md
                radius: Style.cornerRadius
                color: selected ? Style.selectedFillFor(Color.foreground, Color.accent)
                  : (cardMouse.containsMouse ? Style.hoverFillFor(Color.foreground, Color.accent) : (root.stacked ? root.panel : "transparent"))
                border.width: 1
                border.color: selected ? Color.accent : (root.stacked ? (cardMouse.containsMouse ? Util.alpha(Color.foreground, 0.3) : root.faint) : "transparent")

                MouseArea {
                  id: cardMouse
                  anchors.fill: parent
                  hoverEnabled: true
                  cursorShape: Qt.PointingHandCursor
                  onClicked: root.select(card.modelData.id)
                }

                // In a column: one row. In the strip: a card.
                GridLayout {
                  anchors.fill: parent
                  anchors.leftMargin: root.stacked ? Style.spacing.lg : Style.spacing.rowPaddingX
                  anchors.rightMargin: anchors.leftMargin
                  anchors.topMargin: root.stacked ? Style.spacing.lg : 0
                  anchors.bottomMargin: anchors.topMargin
                  flow: root.stacked ? GridLayout.TopToBottom : GridLayout.LeftToRight
                  rows: root.stacked ? 4 : 1
                  columns: root.stacked ? 1 : 4
                  rowSpacing: Style.spacing.xs
                  columnSpacing: Style.spacing.sm

                  RowLayout {
                    Layout.fillWidth: true
                    spacing: Style.spacing.sm
                    Text {
                      text: card.modelData.problem ? Icons.problem : Icons.theme
                      color: card.modelData.problem ? Color.urgent : (card.selected ? Color.accent : root.muted)
                      font.family: root.fontFamily
                      font.pixelSize: Style.font.icon
                    }
                    Text {
                      Layout.fillWidth: true
                      text: card.modelData.name || card.modelData.id
                      color: card.modelData.problem ? Color.urgent : (card.selected ? Color.accent : Color.foreground)
                      font.family: root.fontFamily
                      font.pixelSize: root.stacked ? Style.font.subtitle : Style.font.body
                      font.bold: root.stacked
                      elide: Text.ElideRight
                    }
                  }
                  Text {
                    Layout.fillWidth: root.stacked
                    visible: card.modelData.name && card.modelData.name !== card.modelData.id
                    text: card.modelData.id
                    color: root.muted
                    font.family: root.fontFamily
                    font.pixelSize: Style.font.caption
                    elide: Text.ElideRight
                  }
                  Item { Layout.fillHeight: root.stacked; Layout.fillWidth: !root.stacked; visible: root.stacked }
                  Text {
                    visible: card.modelData.boots
                    text: Icons.boots + " boots"
                    color: Color.accent
                    font.family: root.fontFamily
                    font.pixelSize: Style.font.caption
                  }
                }
              }

              footer: Item {
                width: root.stacked ? newCard.width + Style.spacing.sm : themeList.width
                height: root.stacked ? themeList.height : newCard.height + Style.spacing.sm

                Button {
                  id: newCard
                  anchors.left: parent.left
                  anchors.leftMargin: root.stacked ? Style.spacing.sm : 0
                  anchors.top: parent.top
                  anchors.topMargin: root.stacked ? 0 : Style.spacing.sm
                  width: root.stacked ? 150 : themeList.width
                  height: root.stacked ? root.stripHeight : implicitHeight
                  iconText: Icons.add
                  text: "New theme"
                  bordered: true
                  tooltipText: "From what boots now, from an Omarchy theme, or empty"
                  onClicked: { newDialog.source = root.info && root.info.can_copy_current ? "current" : "blank"; newDialog.openWith("") }
                }
              }

              Text {
                anchors.centerIn: parent
                visible: root.info && root.info.themes.length === 0 && !root.stacked
                width: parent.width - Style.spacing.lg * 2
                text: "No themes of yours yet.\nMake one from what boots now, or from an Omarchy theme."
                color: root.muted
                font.family: root.fontFamily
                font.pixelSize: Style.font.caption
                wrapMode: Text.Wrap
                horizontalAlignment: Text.AlignHCenter
              }
            }
          }

          // ---- the picture --------------------------------------------------
          ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: Style.spacing.md

            RowLayout {
              Layout.fillWidth: true
              spacing: Style.spacing.sm

              Text {
                text: root.onSystem ? Icons.system : Icons.theme
                color: Color.accent
                font.family: root.fontFamily
                font.pixelSize: Style.font.icon
              }
              Text {
                Layout.fillWidth: true
                text: root.onSystem ? "What boots now" : (root.theme ? root.theme.name : root.selection)
                color: Color.foreground
                font.family: root.fontFamily
                font.pixelSize: Style.font.title
                font.bold: true
                elide: Text.ElideRight
              }
              ButtonGroup {
                fontFamily: root.fontFamily
                focusable: false
                spacing: Style.spacing.xs
                options: [
                  { value: "unlock", label: "Unlock", icon: Icons.unlock, tooltip: "Key 1" },
                  { value: "login", label: "Login", icon: Icons.login, tooltip: "Key 2" },
                  { value: "shutdown", label: "Shutdown", icon: Icons.shutdown, tooltip: "Key 3" }
                ]
                value: root.screen
                onChanged: value => root.screen = value
              }
            }

            // The stage: the picture keeps the screen's 16:9, sits at the top
            // of whatever room there is with its caption and buttons right
            // under it, and is the place to drop an image.
            Item {
              id: stage
              Layout.fillWidth: true
              Layout.fillHeight: true
              Layout.minimumHeight: 200

              readonly property real room: height - captionRow.implicitHeight - Style.spacing.md

              Rectangle {
                id: previewFrame
                anchors.horizontalCenter: parent.horizontalCenter
                y: Math.max(0, (stage.height - height - Style.spacing.md - captionRow.implicitHeight) / 2)
                width: Math.max(320, Math.min(stage.width, stage.room * 16 / 9))
                height: width * 9 / 16
                radius: Style.cornerRadius
                color: root.panel
                border.width: dropArea.containsDrag ? 2 : 1
                border.color: dropArea.containsDrag ? Color.accent : Util.alpha(Color.foreground, 0.2)
                clip: true

                readonly property var current: root.previews[root.screen] || null

                Image {
                  id: previewImage
                  anchors.fill: parent
                  anchors.margins: 1
                  fillMode: Image.PreserveAspectFit
                  source: previewFrame.current && previewFrame.current.source ? previewFrame.current.source : ""
                  cache: false
                  asynchronous: true
                  smooth: true
                  visible: previewFrame.current !== null && previewFrame.current.source !== ""
                }

                Text {
                  anchors.centerIn: parent
                  width: parent.width - Style.spacing.panelPadding * 4
                  visible: !previewImage.visible || previewImage.status === Image.Error
                  text: previewImage.status === Image.Error
                    ? "the picture was drawn but could not be shown: " + previewImage.source
                    : (previewFrame.current ? previewFrame.current.message
                      : (root.info ? "drawing…" : (engine.missing ? engine.missingMessage : "")))
                  color: previewFrame.current && previewFrame.current.message ? Color.foreground : root.muted
                  font.family: root.fontFamily
                  font.pixelSize: Style.font.body
                  wrapMode: Text.Wrap
                  horizontalAlignment: Text.AlignHCenter
                }

                Rectangle {
                  anchors.fill: parent
                  visible: dropArea.containsDrag
                  color: Util.alpha(Color.background, 0.7)
                  ColumnLayout {
                    anchors.centerIn: parent
                    spacing: Style.spacing.sm
                    Text {
                      Layout.alignment: Qt.AlignHCenter
                      text: root.screen === "login" ? Icons.wallpaper : Icons.image
                      color: Color.accent
                      font.family: root.fontFamily
                      font.pixelSize: Style.font.display
                    }
                    Text {
                      Layout.alignment: Qt.AlignHCenter
                      text: root.onSystem
                        ? "Make a theme from what boots now first, then drop images on it"
                        : (root.screen === "login" ? "Drop to use as the login wallpaper" : (root.screen === "shutdown" ? "Drop to use as the shutdown logo" : "Drop to use as the logo"))
                      color: Color.accent
                      font.family: root.fontFamily
                      font.pixelSize: Style.font.title
                      font.bold: true
                    }
                  }
                }

                DropArea {
                  id: dropArea
                  anchors.fill: parent
                  keys: ["text/uri-list"]
                  onDropped: drop => {
                    if (!drop.hasUrls || drop.urls.length === 0) { drop.accepted = false; return }
                    var url = String(drop.urls[0])
                    var path = url.indexOf("file://") === 0 ? decodeURIComponent(url.substring(7)) : url
                    drop.accepted = true
                    root.dropFile(path, root.roleForDrop(root.screen, path))
                  }
                }
              }

              RowLayout {
                id: captionRow
                anchors.top: previewFrame.bottom
                anchors.topMargin: Style.spacing.md
                anchors.left: previewFrame.left
                anchors.right: previewFrame.right
                spacing: Style.spacing.sm

                Text {
                  Layout.fillWidth: true
                  text: root.onSystem
                    ? "composite of the installed files, not the real render"
                    : (root.screen === "login"
                        ? "composite; drop a JPG or PNG here for the wallpaper"
                        : (root.screen === "shutdown"
                            ? "composite; drop a PNG or SVG here for the shutdown logo"
                            : "composite; drop a PNG or SVG here for the logo"))
                  color: root.muted
                  font.family: root.fontFamily
                  font.pixelSize: Style.font.caption
                  elide: Text.ElideRight
                }
                Button {
                  iconText: root.screen === "login" ? Icons.wallpaper : Icons.image
                  text: root.screen === "login" ? "Wallpaper" : "Logo"
                  bordered: true
                  visible: !root.onSystem
                  tooltipText: root.screen === "login" ? "Pick a wallpaper for the login screen"
                    : (root.screen === "shutdown" ? "Pick the shutdown logo" : "Pick the logo (PNG or SVG)")
                  enabled: root.busy === ""
                  onClicked: root.chooseImage(root.roleForDrop(root.screen, ""))
                }
                Button {
                  iconText: Icons.show
                  text: "Show for real"
                  bordered: true
                  tooltipText: root.screen === "login"
                    ? "Runs the real greeter in a window with this theme"
                    : "Runs the real plymouthd in a window with this theme; asks for your password once"
                  enabled: root.busy === "" && (root.onSystem || (root.theme && root.theme.valid))
                  onClicked: root.start("preview")
                }
              }
            }
          }

          // ---- properties or facts ------------------------------------------
          ColumnLayout {
            Layout.preferredWidth: root.stacked ? -1 : root.inspectorWidth
            Layout.minimumWidth: root.stacked ? -1 : root.inspectorWidth
            Layout.fillWidth: root.stacked
            Layout.fillHeight: !root.stacked
            spacing: Style.spacing.md

            RowLayout {
              Layout.fillWidth: true
              spacing: Style.spacing.sm
              PanelSectionHeader {
                text: root.onSystem ? "WHAT BOOTS NOW" : "PROPERTIES"
              }
              PanelActionButton {
                visible: !root.onSystem && root.theme !== null
                iconText: Icons.rename
                tooltipText: "Rename"
                onClicked: renameDialog.openWith(root.theme ? root.theme.name : "")
              }
              PanelActionButton {
                visible: !root.onSystem && root.theme !== null
                iconText: Icons.remove
                hoverColor: Color.urgent
                tooltipText: "Delete the theme directory; the system is not touched"
                onClicked: confirm.ask("Delete " + root.selection + "?",
                  "This removes " + (root.theme ? root.theme.dir : "") + " with its theme.toml and images. Nothing installed on the system is touched.",
                  "Delete", function() { root.removeTheme(root.selection) }, Icons.remove)
              }
              Item { Layout.fillWidth: true }
            }

            Flickable {
              id: rightScroll
              Layout.fillWidth: true
              Layout.fillHeight: !root.stacked
              Layout.maximumHeight: root.stacked ? Math.round(window.height * 0.38) : -1
              implicitHeight: rightColumn.implicitHeight
              clip: true
              contentHeight: rightColumn.implicitHeight
              boundsBehavior: Flickable.StopAtBounds

              ColumnLayout {
                id: rightColumn
                width: parent.width
                spacing: Style.spacing.md

                // Facts about the installed screens, each with its source.
                Loader {
                  Layout.fillWidth: true
                  active: root.onSystem && root.info !== null
                  visible: active
                  sourceComponent: Facts {
                    info: root.info
                    screen: root.screen
                    fontFamily: root.fontFamily
                    muted: root.muted
                    columns: root.stacked ? Math.max(1, Math.floor((window.width - Style.spacing.panelPadding * 2) / 280)) : 1
                  }
                }

                // The editable properties of the selected theme.
                Loader {
                  Layout.fillWidth: true
                  active: !root.onSystem && root.theme !== null
                  visible: active
                  sourceComponent: Properties {
                    theme: root.theme
                    screen: root.screen
                    fontFamily: root.fontFamily
                    muted: root.muted
                    columns: root.stacked ? Math.max(1, Math.floor((window.width - Style.spacing.panelPadding * 2) / 340)) : 1
                    onEdited: (key, value) => root.setField(key, value)
                    onPickColour: (key, current) => colourPopup.openFor(key, current)
                    onChooseFile: role => root.chooseImage(role)
                  }
                }
              }
            }

            // Try it, then do it.
            RowLayout {
              Layout.fillWidth: true
              spacing: Style.spacing.sm
              visible: !root.onSystem && root.theme !== null

              Item { Layout.fillWidth: true; visible: root.stacked }
              Button {
                iconText: Icons.dryrun
                text: "Dry run"
                bordered: true
                tooltipText: "Lists every operation an apply would perform and performs none"
                enabled: root.theme && root.theme.valid && root.busy === ""
                onClicked: root.start("dryrun")
              }
              Button {
                Layout.fillWidth: !root.stacked
                iconText: Icons.apply
                text: "Apply to the system"
                bordered: true
                accent: Color.accent
                foreground: Color.accent
                tooltipText: "Installs this theme next to Omarchy's, points Plymouth and SDDM at it and rebuilds the initramfs"
                enabled: root.theme && root.theme.valid && root.busy === ""
                onClicked: confirm.ask("Apply " + root.selection + " to the system?",
                  "Installs into /usr/share/plymouth/themes/omaboot and /usr/share/sddm/themes/omaboot, writes /etc/sddm.conf.d/zz-omaboot.conf, sets the Plymouth default to omaboot and rebuilds the initramfs. Never writes to the directories Omarchy owns. You will be asked for your password once. Undo it with Revert.",
                  "Apply", function() { root.start("apply") }, Icons.apply)
              }
            }

            // What can be done to the system itself, in the order it comes up.
            GridLayout {
              Layout.fillWidth: true
              visible: root.onSystem
              flow: root.stacked ? GridLayout.LeftToRight : GridLayout.TopToBottom
              columns: root.stacked ? 4 : 1
              rows: root.stacked ? 1 : 4
              rowSpacing: Style.spacing.sm
              columnSpacing: Style.spacing.sm

              // The login screen belongs to another theme's drop-in: offer
              // Omarchy's own login screen, with omaboot's drop-in outranking
              // it, and the way back.
              Button {
                Layout.fillWidth: !root.stacked
                iconText: Icons.login
                text: "Use Omarchy's login screen"
                bordered: true
                leftAlign: true
                visible: !!(root.info && root.info.login.owner === "other" && !root.info.login.by_omaboot)
                tooltipText: "Writes /etc/sddm.conf.d/zz-omaboot.conf with Current=omarchy, which outranks the drop-in that chose " + (root.info ? root.info.login.theme : "") + ". Nothing else is touched; asks for your password once."
                onClicked: confirm.ask("Use Omarchy's login screen?",
                  "The login screen is " + (root.info ? root.info.login.theme : "") + ", chosen by " + (root.info ? root.info.login.decided_by : "") + ". omaboot writes its own drop-in, /etc/sddm.conf.d/zz-omaboot.conf, saying Current=omarchy; it sorts last, so it wins. That file is not touched, Plymouth is not touched, no initramfs is rebuilt. \"Give the login screen back\" removes the drop-in again.",
                  "Use Omarchy's", function() { root.start("login-stock") }, Icons.login)
              }
              Button {
                Layout.fillWidth: !root.stacked
                iconText: Icons.release
                text: "Give the login screen back"
                bordered: true
                leftAlign: true
                visible: !!(root.info && root.info.login.by_omaboot && !(root.info.applied && root.info.applied.current))
                tooltipText: "Removes /etc/sddm.conf.d/zz-omaboot.conf, so whatever else is configured decides the login screen again"
                onClicked: confirm.ask("Give the login screen back?",
                  "Removes /etc/sddm.conf.d/zz-omaboot.conf. The next drop-in in line decides the login screen again. Nothing else changes.",
                  "Remove the drop-in", function() { root.start("login-release") }, Icons.release)
              }
              Button {
                Layout.fillWidth: !root.stacked
                iconText: Icons.revert
                text: "Revert the last apply"
                bordered: true
                leftAlign: true
                visible: !!(root.info && root.info.rollback)
                tooltipText: root.info && root.info.rollback ? "Puts back the Plymouth theme and SDDM drop-in recorded " + root.info.rollback.age + " and rebuilds the initramfs" : ""
                onClicked: confirm.ask("Revert the last apply?",
                  "Restores the Plymouth theme that was default before (" + (root.info && root.info.rollback ? (root.info.rollback.previous_plymouth_theme || "not set") : "") + "), removes or restores the SDDM drop-in, and rebuilds the initramfs. Your themes stay as they are.",
                  "Revert", function() { root.start("revert") }, Icons.revert)
              }
              Button {
                Layout.fillWidth: !root.stacked
                iconText: Icons.reset
                text: "Remove omaboot from the system"
                bordered: true
                leftAlign: true
                visible: !!(root.info && (root.info.applied || root.info.rollback))
                tooltipText: "Back to Omarchy's own themes: removes the omaboot theme directories, the drop-in and the state, rebuilds the initramfs"
                onClicked: confirm.ask("Remove omaboot from the system?",
                  "Sets Plymouth back to omarchy, removes /etc/sddm.conf.d/zz-omaboot.conf and the omaboot theme directories, rebuilds the initramfs, and clears the applied state. Your themes in ~/.config/omaboot stay.",
                  "Remove", function() { root.start("reset") }, Icons.reset)
              }
            }
          }
        }

        // Footer: status, warnings.
        ColumnLayout {
          Layout.fillWidth: true
          spacing: Style.spacing.xs
          Repeater {
            model: root.info ? root.info.warnings : []
            Text {
              required property var modelData
              Layout.fillWidth: true
              text: Icons.problem + "  " + modelData
              color: Color.urgent
              font.family: root.fontFamily
              font.pixelSize: Style.font.caption
              wrapMode: Text.Wrap
            }
          }
          Text {
            Layout.fillWidth: true
            text: root.status
            color: root.statusIsError ? Color.urgent : root.muted
            font.family: root.fontFamily
            font.pixelSize: Style.font.caption
            wrapMode: Text.Wrap
          }
        }
      }

      // ---- overlays --------------------------------------------------------

      ColourPopup {
        id: colourPopup
        anchors.fill: parent
        fontFamily: root.fontFamily
        omarchyThemes: root.info ? root.info.omarchy_themes : []
        activeOmarchyTheme: root.info ? root.info.active_omarchy_theme : ""
        wallpaperPalette: root.wallpaperPalette
        onPicked: (key, value) => root.setField(key, value)
      }

      Modal {
        id: newDialog
        anchors.fill: parent
        title: "New theme"
        icon: Icons.add
        fontFamily: root.fontFamily
        property string source: "current"
        property string problem: ""
        function openWith(name) { text = name; problem = ""; opened = true }

        content: ColumnLayout {
          spacing: Style.spacing.md
          width: parent ? parent.width : 400

          Text { text: "Name"; color: root.muted; font.family: root.fontFamily; font.pixelSize: Style.font.caption }
          TextField {
            id: newName
            Layout.fillWidth: true
            placeholderText: "for example my-theme"
            onTextChanged: newDialog.text = text
            onAccepted: root.makeTheme(newDialog.text, newDialog.source)
            Component.onCompleted: { text = newDialog.text; forceActiveFocus() }
          }
          Text { text: "Start from"; color: root.muted; font.family: root.fontFamily; font.pixelSize: Style.font.caption }
          Dropdown {
            Layout.fillWidth: true
            showLabel: false
            value: newDialog.source
            options: {
              var list = []
              if (root.info && root.info.can_copy_current) list.push({ value: "current", label: "the screens that boot now" })
              var themes = root.info ? root.info.omarchy_themes : []
              for (var i = 0; i < themes.length; i++) list.push({ value: themes[i].name, label: "Omarchy theme " + themes[i].name })
              list.push({ value: "blank", label: "empty, add your own logo" })
              return list
            }
            onChanged: value => newDialog.source = value
          }
          Text {
            Layout.fillWidth: true
            visible: newDialog.problem !== ""
            text: newDialog.problem
            color: Color.urgent
            font.family: root.fontFamily
            font.pixelSize: Style.font.caption
            wrapMode: Text.Wrap
          }
          RowLayout {
            Layout.fillWidth: true
            Item { Layout.fillWidth: true }
            Button { text: "Cancel"; bordered: true; onClicked: newDialog.opened = false }
            Button {
              text: "Create"
              bordered: true
              foreground: Color.accent
              enabled: newDialog.text.trim() !== ""
              onClicked: root.makeTheme(newDialog.text, newDialog.source)
            }
          }
        }
      }

      Modal {
        id: renameDialog
        anchors.fill: parent
        title: "Rename"
        icon: Icons.rename
        fontFamily: root.fontFamily
        function openWith(name) { text = name; opened = true }
        function commit() {
          var id = root.selection
          var name = renameDialog.text
          root.whenIdle(function() {
            root.saving = true
            engine.rename(id, name, function(ok, data, message) {
              if (!ok) root.say(message, true)
              else {
                renameDialog.opened = false
                if (root.selection === id) root.theme = data
                root.refresh(true)
              }
              root.saveDone()
            })
          })
        }
        content: ColumnLayout {
          spacing: Style.spacing.md
          width: parent ? parent.width : 400
          TextField {
            Layout.fillWidth: true
            onTextChanged: renameDialog.text = text
            onAccepted: renameDialog.commit()
            Component.onCompleted: { text = renameDialog.text; forceActiveFocus(); selectAll() }
          }
          RowLayout {
            Layout.fillWidth: true
            Item { Layout.fillWidth: true }
            Button { text: "Cancel"; bordered: true; onClicked: renameDialog.opened = false }
            Button { text: "Rename"; bordered: true; foreground: Color.accent; onClicked: renameDialog.commit() }
          }
        }
      }

      Modal {
        id: passwordDialog
        anchors.fill: parent
        title: "Your password"
        icon: Icons.password
        fontFamily: root.fontFamily
        property string problem: ""
        function commit() {
          var pw = text
          text = ""
          opened = false
          root.go(root.pendingAction, pw)
          pw = ""
        }
        content: ColumnLayout {
          spacing: Style.spacing.md
          width: parent ? parent.width : 400
          Text {
            Layout.fillWidth: true
            text: root.pendingAction === "preview"
              ? "plymouthd only runs as root, so showing the real screen needs sudo once."
              : "Changing what boots needs sudo once. The password goes to sudo and is not kept."
            color: root.muted
            font.family: root.fontFamily
            font.pixelSize: Style.font.caption
            wrapMode: Text.Wrap
          }
          TextField {
            Layout.fillWidth: true
            password: true
            onTextChanged: passwordDialog.text = text
            onAccepted: passwordDialog.commit()
            Component.onCompleted: forceActiveFocus()
          }
          Text {
            Layout.fillWidth: true
            visible: passwordDialog.problem !== ""
            text: passwordDialog.problem
            color: Color.urgent
            font.family: root.fontFamily
            font.pixelSize: Style.font.caption
            wrapMode: Text.Wrap
          }
          RowLayout {
            Layout.fillWidth: true
            Item { Layout.fillWidth: true }
            Button { text: "Cancel"; bordered: true; onClicked: { passwordDialog.text = ""; passwordDialog.opened = false } }
            Button { text: "Continue"; bordered: true; foreground: Color.accent; enabled: passwordDialog.text !== ""; onClicked: passwordDialog.commit() }
          }
        }
      }

      Modal {
        id: confirm
        anchors.fill: parent
        fontFamily: root.fontFamily
        property string message: ""
        property string action: "Go ahead"
        property var onConfirm: null
        function ask(question, body, verb, callback, glyph) {
          title = question; message = body; action = verb; onConfirm = callback; icon = glyph || Icons.info; opened = true
        }
        content: ColumnLayout {
          spacing: Style.spacing.md
          width: parent ? parent.width : 400
          Text {
            Layout.fillWidth: true
            text: confirm.message
            color: Color.foreground
            font.family: root.fontFamily
            font.pixelSize: Style.font.body
            wrapMode: Text.Wrap
          }
          RowLayout {
            Layout.fillWidth: true
            Item { Layout.fillWidth: true }
            Button { text: "Leave it"; bordered: true; onClicked: confirm.opened = false }
            Button {
              text: confirm.action
              bordered: true
              foreground: Color.accent
              onClicked: { confirm.opened = false; if (confirm.onConfirm) confirm.onConfirm() }
            }
          }
        }
      }

      RunView {
        id: runView
        anchors.fill: parent
        fontFamily: root.fontFamily
        muted: root.muted
        onClosed: {
          root.busy = ""
          root.previews = ({})
          root.refresh(true)
        }
        onPasswordRefused: {
          // sudo said no: the run never started, so ask again.
          passwordDialog.problem = "the password was not accepted"
          passwordDialog.opened = true
        }
      }
    }
  }

  // One line of the system card: a glyph that says which screen, the value,
  // and a tooltip that says it in words.
  component FactLine: Item {
    id: line
    property string icon: ""
    property string text: ""
    property string tip: ""
    property color colour: root.muted
    Layout.fillWidth: true
    implicitHeight: lineRow.implicitHeight

    RowLayout {
      id: lineRow
      anchors.left: parent.left
      anchors.right: parent.right
      spacing: Style.spacing.sm

      Text {
        text: line.icon
        color: line.colour
        font.family: root.fontFamily
        font.pixelSize: Style.font.iconSmall
      }
      Text {
        Layout.fillWidth: true
        text: line.text
        color: line.colour
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
        elide: Text.ElideRight
      }
    }
    MouseArea {
      id: factHover
      anchors.fill: parent
      hoverEnabled: true
      acceptedButtons: Qt.NoButton
    }
    PanelToolTip {
      visible: line.tip !== "" && factHover.containsMouse
      text: line.tip
      fontFamily: root.fontFamily
    }
  }
}
