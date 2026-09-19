import QtQuick
import Quickshell
import Quickshell.Io

// The bridge to the omaboot binary.
//
// Every call is argv only: no shell, no string interpolation, no environment
// beyond what the shell already has. The engine answers in JSON, one object
// per line, and this component turns those lines into callbacks. Nothing in
// the plugin reads a theme file or touches the system directly; if a rule
// exists it exists in the engine, once.
//
// Calls run concurrently, each in its own Process, so three previews can be
// drawn while a theme is being saved.
Item {
  id: root

  // Where the binary is. `omaboot plugin install` links it into
  // ~/.local/bin; a packaged install is on PATH. `probe` picks between the
  // two once, with `test -x`, before the first call.
  readonly property string home: Quickshell.env("HOME")
  readonly property string linked: home + "/.local/bin/omaboot"
  property string binary: linked
  property bool probed: false
  property var queued: []

  signal ready()

  Process {
    id: probe
    command: ["test", "-x", root.linked]
    onExited: (exitCode, exitStatus) => {
      root.binary = exitCode === 0 ? root.linked : "omaboot"
      root.probed = true
      var pending = root.queued
      root.queued = []
      for (var i = 0; i < pending.length; i++) root.spawn(pending[i][0], pending[i][1], pending[i][2], pending[i][3])
      root.ready()
    }
  }

  Component.onCompleted: probe.running = true

  // Where previews are written: a per-session tmpfs the engine can write and
  // the window can read, cleared at logout.
  readonly property string runtimeDir: (Quickshell.env("XDG_RUNTIME_DIR") || "/tmp") + "/omaboot"

  property int running: 0
  property string lastError: ""

  signal failed(string message)

  Component {
    id: processComponent

    Process {
      id: proc
      property var onLine: null
      property var onDone: null
      property string stdinText: ""
      property bool hasStdin: false
      property var lines: []
      property var lastObject: null
      property string errorMessage: ""
      property string stderrText: ""

      stdinEnabled: hasStdin

      stdout: SplitParser {
        splitMarker: "\n"
        onRead: data => {
          var text = String(data).trim()
          if (text.length === 0) return
          var obj = null
          try { obj = JSON.parse(text) } catch (e) { obj = { event: "text", message: text } }
          proc.lines.push(obj)
          proc.lastObject = obj
          if (obj && obj.event === "error") proc.errorMessage = String(obj.message || "")
          if (proc.onLine) {
            try { proc.onLine(obj) } catch (e) { console.warn("omaboot: onLine threw:", e) }
          }
        }
      }

      stderr: SplitParser {
        splitMarker: "\n"
        onRead: data => {
          var text = String(data).trim()
          if (text.length > 0) proc.stderrText = (proc.stderrText ? proc.stderrText + "\n" : "") + text
        }
      }

      onStarted: {
        if (stdinText.length > 0) {
          proc.write(stdinText + "\n")
          // The password is in the engine now; nothing here needs it again.
          stdinText = ""
        }
      }

      onExited: (exitCode, exitStatus) => {
        root.running = Math.max(0, root.running - 1)
        var ok = exitCode === 0 && proc.errorMessage.length === 0
        var message = proc.errorMessage
        if (!ok && message.length === 0) {
          message = proc.stderrText.split("\n").filter(function(l) { return l.indexOf("omaboot:") === 0 }).pop()
            || proc.stderrText.split("\n").pop()
            || ("omaboot exited with code " + exitCode)
          message = message.replace(/^omaboot:\s*/, "")
        }
        if (!ok) root.lastError = message
        if (proc.onDone) {
          try { proc.onDone(ok, proc.lastObject, message, proc.lines) } catch (e) { console.warn("omaboot: onDone threw:", e) }
        }
        if (!ok) root.failed(message)
        proc.destroy()
      }
    }
  }

  // Run `omaboot <args> --json`. `onLine(obj)` sees every JSON line as it
  // arrives; `onDone(ok, last, message, lines)` runs once at the end.
  function call(args, onDone, onLine, stdinText) {
    spawn(args, onLine || null, onDone || null, stdinText || "")
  }

  function spawn(args, onLine, onDone, stdinText) {
    if (!probed) {
      queued.push([args, onLine, onDone, stdinText])
      return null
    }
    var command = [binary].concat(args).concat(["--json"])
    if (stdinText && stdinText.length > 0) command.push("--password-stdin")
    var proc = processComponent.createObject(root, {
      command: command,
      onLine: onLine,
      onDone: onDone,
      stdinText: stdinText,
      hasStdin: stdinText.length > 0
    })
    root.running += 1
    proc.running = true
    return proc
  }

  // Convenience wrappers, so the rest of the plugin reads as intent.
  function inspect(onDone) { call(["status"], onDone) }
  function show(theme, onDone) { call(["show", theme], onDone) }
  function set(theme, assignments, onDone) { call(["set", theme].concat(assignments), onDone) }
  function rename(theme, name, onDone) { call(["rename", theme, name], onDone) }
  function addImage(theme, file, role, onDone) { call(["add-image", theme, file, "--as", role], onDone) }
  function palette(file, onDone) { call(["palette", file], onDone) }
  function newTheme(name, source, onDone) {
    var args = ["new", name]
    if (source === "current") args.push("--from-current")
    else if (source && source !== "blank") args = args.concat(["--from-omarchy-theme", source])
    call(args, onDone)
  }
  function render(theme, screen, out, size, onDone) {
    var args = ["render"]
    if (theme) args.push(theme); else args.push("--current")
    call(args.concat(["--screen", screen, "--out", out, "--size", size || "960x540"]), onDone)
  }
  function apply(theme, dryRun, password, onLine, onDone) {
    var args = ["apply", theme]
    if (dryRun) args.push("--dry-run")
    call(args, onDone, onLine, dryRun ? "" : password)
  }
  function revert(password, onLine, onDone) { call(["revert"], onDone, onLine, password) }
  function reset(password, onLine, onDone) { call(["reset"], onDone, onLine, password) }
  function preview(theme, screen, password, onLine, onDone) {
    var args = ["preview"]
    if (theme) args.push(theme); else args.push("--current")
    args = args.concat(["--screen", screen, "--seconds", "120"])
    // The greeter needs no privilege; plymouthd does.
    call(args, onDone, onLine, screen === "login" ? "" : password)
  }
  function deleteTheme(theme, onDone) { call(["delete", theme], onDone) }
  function login(what, password, onLine, onDone) { call(["login", what], onDone, onLine, password) }
  function setPrefs(assignments, onDone) { call(["prefs"].concat(assignments), onDone) }
}
