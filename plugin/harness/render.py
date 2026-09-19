#!/usr/bin/env python3
"""Render the omaboot window to a PNG without a shell, a display or root.

The plugin is plain QML on top of the shell's `qs.Commons` and `qs.Ui`
modules and a handful of Quickshell types. This harness supplies those
Quickshell types in Python (Process on QProcess, FileView, a `Quickshell`
singleton whose `env` answers from a fake home) and loads the real shell
modules from an Omarchy checkout, so the window renders with the real
widgets, tokens and font, offscreen, against the real engine running in a
sandbox prefix. What comes out is what the shell would draw, minus Hyprland's
rounding and gaps, which the shell reads from hyprctl and this harness leaves
at their defaults.

    plugin/harness/render.py --out /tmp/omaboot.png
    plugin/harness/render.py --theme tokyo-night --size 1600x1000 --screen login --out /tmp/login.png
    plugin/harness/render.py --script 'root.select("now")' --out /tmp/system.png

Needs: PySide6 (`pip install PySide6`), a Nerd Font fontconfig resolves for
`monospace`, `target/release/omaboot`, and an Omarchy tree (`OMARCHY_PATH`,
or `--omarchy`) with `shell/`, `themes/` and `default/`.
"""

import argparse
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
os.environ.setdefault("QT_LOGGING_RULES", "qt.qml.connections=false")

from PySide6.QtCore import (  # noqa: E402
    Property,
    qInstallMessageHandler,
    QObject,
    QProcess,
    QTimer,
    QUrl,
    Signal,
    Slot,
)
from PySide6.QtGui import QGuiApplication  # noqa: E402
from PySide6.QtQml import (  # noqa: E402
    QQmlApplicationEngine,
    QQmlEngine,
    QQmlExpression,
    qmlRegisterSingletonInstance,
    qmlRegisterType,
)
from PySide6.QtQuick import QQuickWindow, QSGRendererInterface  # noqa: E402

HERE = Path(__file__).resolve().parent
REPO = HERE.parent.parent


# ---------------------------------------------------------------- Quickshell

class HarnessEnv(QObject):
    """The `Quickshell` singleton: `env(name)` from the harness's own map."""

    def __init__(self, env):
        super().__init__()
        self._env = env

    @Slot(str, result=str)
    def env(self, name):
        return self._env.get(name, os.environ.get(name, ""))

    @Slot("QVariantList")
    def execDetached(self, argv):
        # What Quickshell does: run it and forget it. Scripts use it to
        # change the world between two looks at the window.
        subprocess.Popen([str(a) for a in argv])


class SplitParser(QObject):
    read = Signal(str)
    splitMarkerChanged = Signal()

    def __init__(self, parent=None):
        super().__init__(parent)
        self._marker = "\n"
        self._buffer = ""

    def _get_marker(self):
        return self._marker

    def _set_marker(self, value):
        self._marker = value
        self.splitMarkerChanged.emit()

    splitMarker = Property(str, _get_marker, _set_marker, notify=splitMarkerChanged)

    def feed(self, text):
        self._buffer += text
        while self._marker and self._marker in self._buffer:
            line, self._buffer = self._buffer.split(self._marker, 1)
            self.read.emit(line)

    def finish(self):
        if self._buffer:
            self.read.emit(self._buffer)
            self._buffer = ""


class StdioCollector(QObject):
    streamFinished = Signal()
    textChanged = Signal()
    waitForEndChanged = Signal()

    def __init__(self, parent=None):
        super().__init__(parent)
        self._text = ""
        self._wait = False

    def _get_text(self):
        return self._text

    text = Property(str, _get_text, notify=textChanged)

    def _get_wait(self):
        return self._wait

    def _set_wait(self, value):
        self._wait = value
        self.waitForEndChanged.emit()

    waitForEnd = Property(bool, _get_wait, _set_wait, notify=waitForEndChanged)

    def feed(self, text):
        self._text += text
        self.textChanged.emit()

    def finish(self):
        self.streamFinished.emit()


class Process(QObject):
    """Quickshell's Process on QProcess: command, running, stdin, parsers."""

    commandChanged = Signal()
    runningChanged = Signal()
    stdinEnabledChanged = Signal()
    stdoutChanged = Signal()
    stderrChanged = Signal()
    started = Signal()
    exited = Signal(int, int)

    env = {}
    # QML creates these; keep the Python side alive for as long as the
    # QObject is, or PySide drops the slot connections with the wrapper.
    instances = []

    def __init__(self, parent=None):
        super().__init__(parent)
        Process.instances.append(self)
        self.destroyed.connect(lambda: Process.instances.remove(self) if self in Process.instances else None)
        self._command = []
        self._running = False
        self._stdin = False
        self._stdout = None
        self._stderr = None
        self._proc = None

    def _get_command(self):
        return self._command

    def _set_command(self, value):
        self._command = [str(v) for v in value]
        self.commandChanged.emit()

    command = Property("QVariantList", _get_command, _set_command, notify=commandChanged)

    def _get_running(self):
        return self._running

    def _set_running(self, value):
        if value and not self._running:
            self._start()
        elif not value and self._running and self._proc:
            self._proc.kill()

    running = Property(bool, _get_running, _set_running, notify=runningChanged)

    def _get_stdin(self):
        return self._stdin

    def _set_stdin(self, value):
        self._stdin = value
        self.stdinEnabledChanged.emit()

    stdinEnabled = Property(bool, _get_stdin, _set_stdin, notify=stdinEnabledChanged)

    def _get_stdout(self):
        return self._stdout

    def _set_stdout(self, value):
        self._stdout = value
        self.stdoutChanged.emit()

    stdout = Property(QObject, _get_stdout, _set_stdout, notify=stdoutChanged)

    def _get_stderr(self):
        return self._stderr

    def _set_stderr(self, value):
        self._stderr = value
        self.stderrChanged.emit()

    stderr = Property(QObject, _get_stderr, _set_stderr, notify=stderrChanged)

    @Slot(str)
    def write(self, text):
        if self._proc:
            self._proc.write(text.encode())

    def _start(self):
        if not self._command:
            return
        self._proc = QProcess(self)
        env = self._proc.processEnvironment()
        for key, value in Process.env.items():
            env.insert(key, value)
        self._proc.setProcessEnvironment(env)
        # Lambdas, not bound methods: PySide keeps only a weak reference to
        # a bound-method slot on a QML-created object, and drops it.
        me = self
        self._proc.readyReadStandardOutput.connect(lambda: me._read_out())
        self._proc.readyReadStandardError.connect(lambda: me._read_err())
        self._proc.finished.connect(lambda code, status: me._finished(code, status))
        self._proc.errorOccurred.connect(lambda error: me._error(error))
        self._running = True
        self.runningChanged.emit()
        if os.environ.get("HARNESS_TRACE"):
            print("process:", " ".join(self._command), file=sys.stderr)
        self._proc.start(self._command[0], self._command[1:])
        if not self._proc.waitForStarted(5000):
            if os.environ.get("HARNESS_TRACE"):
                print("not started:", self._command[0], self._proc.error(), file=sys.stderr)
            return
        if os.environ.get("HARNESS_TRACE"):
            print("running: pid", self._proc.processId(), self._command[0], file=sys.stderr)
        self.started.emit()
        if not self._stdin:
            self._proc.closeWriteChannel()

    def _feed(self, sink, data):
        if sink is None:
            return
        if hasattr(sink, "feed"):
            sink.feed(bytes(data).decode(errors="replace"))

    def _read_out(self):
        data = self._proc.readAllStandardOutput()
        if os.environ.get("HARNESS_TRACE"):
            print("stdout:", len(data), "bytes from", self._command[0], file=sys.stderr)
        self._feed(self._stdout, data)

    def _read_err(self):
        self._feed(self._stderr, self._proc.readAllStandardError())

    def _error(self, error):
        if os.environ.get("HARNESS_TRACE"):
            print("error:", error, " ".join(self._command[:3]), file=sys.stderr)
        if error == QProcess.ProcessError.FailedToStart:
            self._finished(127, QProcess.ExitStatus.CrashExit)

    def _finished(self, code, status):
        if os.environ.get("HARNESS_TRACE"):
            print("finished:", code, status, self._running, " ".join(self._command[:3]), file=sys.stderr)
        if not self._running:
            return
        self._read_out()
        self._read_err()
        for sink in (self._stdout, self._stderr):
            if sink is not None and hasattr(sink, "finish"):
                sink.finish()
        self._running = False
        self.runningChanged.emit()
        if os.environ.get("HARNESS_TRACE"):
            print("exited:", code, " ".join(self._command[:3]), file=sys.stderr)
        self.exited.emit(int(code), 0 if status == QProcess.ExitStatus.NormalExit else 1)


class FileView(QObject):
    pathChanged = Signal()
    watchChangesChanged = Signal()
    printErrorsChanged = Signal()
    loaded = Signal()
    loadFailed = Signal()
    fileChanged = Signal()

    def __init__(self, parent=None):
        super().__init__(parent)
        self._path = ""
        self._text = ""
        self._watch = False
        self._print = False

    def _get_path(self):
        return self._path

    def _set_path(self, value):
        self._path = value
        self.pathChanged.emit()
        QTimer.singleShot(0, self.reload)

    path = Property(str, _get_path, _set_path, notify=pathChanged)

    def _get_watch(self):
        return self._watch

    def _set_watch(self, value):
        self._watch = value
        self.watchChangesChanged.emit()

    watchChanges = Property(bool, _get_watch, _set_watch, notify=watchChangesChanged)

    def _get_print(self):
        return self._print

    def _set_print(self, value):
        self._print = value
        self.printErrorsChanged.emit()

    printErrors = Property(bool, _get_print, _set_print, notify=printErrorsChanged)

    @Slot(result=str)
    def text(self):
        return self._text

    @Slot()
    def reload(self):
        try:
            self._text = Path(self._path).read_text()
        except OSError:
            self._text = ""
            self.loadFailed.emit()
            return
        self.loaded.emit()


# ---------------------------------------------------------------- the world

def render_shell_toml(template, colors):
    """The shell.toml Omarchy generates from a theme, enough for the tokens."""
    import re

    def value(key):
        return colors.get(key, "#888888")

    def sub(match):
        expr = match.group(1).strip().split()
        if expr[0] == "shell_gradient":
            return value(expr[2])
        if expr[0] == "mix":
            return value(expr[1])
        return value(expr[0])

    return re.sub(r"\{\{([^}]*)\}\}", sub, template)


def read_colors(path):
    colors = {}
    for line in path.read_text().splitlines():
        if "=" in line and not line.strip().startswith("#"):
            key, _, rest = line.partition("=")
            colors[key.strip()] = rest.strip().strip('"')
    return colors


def build_world(work, omarchy, theme, engine_binary):
    """A fake home, a sandbox prefix, the Omarchy theme, and a theme of ours."""
    home = work / "home"
    prefix = work / "prefix"
    runtime = work / "run"
    for d in (home / ".local/bin", home / ".local/state/omarchy/current", home / ".config/omaboot",
              home / ".config/fontconfig", prefix / "usr/bin", prefix / "etc/plymouth",
              prefix / "etc/sddm.conf.d", prefix / "usr/share/plymouth/themes",
              prefix / "usr/share/sddm/themes", runtime):
        d.mkdir(parents=True, exist_ok=True)

    theme_dir = omarchy / "themes" / theme
    if not theme_dir.is_dir():
        sys.exit(f"no such Omarchy theme: {theme_dir}")
    current = home / ".local/state/omarchy/current/theme"
    if current.is_symlink() or current.exists():
        current.unlink()
    current.symlink_to(theme_dir)
    # The generated shell.toml lives beside colors.toml on a real system.
    tpl = omarchy / "default/themed/shell.toml.tpl"
    if tpl.is_file() and not (theme_dir / "shell.toml").exists():
        generated = work / "shell.toml"
        generated.write_text(render_shell_toml(tpl.read_text(), read_colors(theme_dir / "colors.toml")))
        shadow = work / "themes" / theme
        shadow.parent.mkdir(exist_ok=True)
        if shadow.exists():
            shutil.rmtree(shadow)
        shutil.copytree(theme_dir, shadow, symlinks=True)
        shutil.copy(generated, shadow / "shell.toml")
        current.unlink()
        current.symlink_to(shadow)

    # The Omarchy tree itself. Under --root the engine reads it at
    # <prefix>/usr/share/omarchy and never at $OMARCHY_PATH, so the tree the
    # harness was given is linked in there.
    tree = prefix / "usr/share/omarchy"
    if tree.is_symlink() or tree.exists():
        tree.unlink()
    tree.symlink_to(omarchy)

    # The stock system: Omarchy's Plymouth and SDDM themes, as installed.
    stock_plymouth = prefix / "usr/share/plymouth/themes/omarchy"
    if not stock_plymouth.exists():
        shutil.copytree(omarchy / "default/plymouth", stock_plymouth)
    stock_sddm = prefix / "usr/share/sddm/themes/omarchy"
    if not stock_sddm.exists():
        shutil.copytree(omarchy / "default/sddm/omarchy", stock_sddm)
    (prefix / "etc/plymouth/plymouthd.conf").write_text("[Daemon]\nTheme=omarchy\n")
    (prefix / "etc/sddm.conf.d/99-omarchy-login.conf").write_text("[Theme]\nCurrent=omarchy\n")
    for tool in ("sddm-greeter-qt6", "limine-mkinitcpio", "plymouth-set-default-theme"):
        t = prefix / "usr/bin" / tool
        t.write_text("#!/bin/sh\nexit 0\n")
        t.chmod(0o755)

    # The wrapper is what the harness scripts and the window's own Process
    # calls run. The engine prefers XDG_CONFIG_HOME and XDG_STATE_HOME over
    # HOME, and a login session exports both, so exporting HOME alone let the
    # engine read and write the real ~/.config/omaboot from inside a harness
    # run. Every variable the engine's Layout reads is set here.
    wrapper = home / ".local/bin/omaboot"
    wrapper.write_text(
        "#!/bin/sh\n"
        f"export HOME={home} XDG_CONFIG_HOME={home / '.config'} XDG_STATE_HOME={home / '.local/state'}\n"
        f"export XDG_RUNTIME_DIR={runtime} OMARCHY_PATH={omarchy}\n"
        f"exec {engine_binary} --root {prefix} \"$@\"\n"
    )
    wrapper.chmod(0o755)
    return home, prefix, runtime, wrapper


def seed_theme(wrapper, name, omarchy_theme, title):
    if subprocess.run([str(wrapper), "show", name], capture_output=True).returncode == 0:
        return
    subprocess.run([str(wrapper), "new", name, "--from-omarchy-theme", omarchy_theme], check=True,
                   capture_output=True)
    subprocess.run([str(wrapper), "rename", name, title], check=True, capture_output=True)


def build_imports(work, shell):
    imports = work / "imports"
    qs = imports / "qs"
    qs.mkdir(parents=True, exist_ok=True)
    for name in ("Commons", "Ui"):
        link = qs / name
        if link.is_symlink() or link.exists():
            link.unlink()
        link.symlink_to(shell / name)
    return imports


# ---------------------------------------------------------------- main

def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--out", required=True, help="PNG to write")
    parser.add_argument("--omarchy", default=os.environ.get("OMARCHY_PATH", ""),
                        help="Omarchy tree with shell/, themes/, default/ (default: $OMARCHY_PATH)")
    parser.add_argument("--shell", default="", help="the shell QML tree (default: <omarchy>/shell)")
    parser.add_argument("--engine", default=str(REPO / "target/release/omaboot"))
    parser.add_argument("--work", default=str(REPO / ".harness"),
                        help="where the fake home and prefix live; outside plugin/, so the shell's"
                             " file watcher does not reload the linked plugin on every run")
    parser.add_argument("--theme", default="matte-black", help="the Omarchy theme the desktop is on")
    parser.add_argument("--screen", choices=["unlock", "login", "shutdown"], default=None)
    parser.add_argument("--select", default=None, help='"now" or a theme id')
    parser.add_argument("--script", action="append", default=[],
                        help="JavaScript to run against the window's root item before the grab")
    parser.add_argument("--size", default="1240x800")
    parser.add_argument("--entry", default=str(REPO / "plugin/Omaboot.qml"),
                        help="the QML to load; the plugin by default, or a generated app's shell.qml")
    parser.add_argument("--app", default="app",
                        help="with an entry that is not the plugin, the id of the Omaboot item inside it")
    parser.add_argument("--settle", type=float, default=0.6, help="seconds to wait after the window is idle")
    parser.add_argument("--script-pause", type=float, default=0, help="seconds to wait after each script")
    parser.add_argument("--timeout", type=float, default=30)
    parser.add_argument("--expect-status", default=None, help="fail unless the status line contains this")
    args = parser.parse_args()

    omarchy = Path(args.omarchy).expanduser().resolve() if args.omarchy else None
    if not omarchy or not omarchy.is_dir():
        sys.exit("give --omarchy or set OMARCHY_PATH to an Omarchy tree")
    shell = Path(args.shell).resolve() if args.shell else omarchy / "shell"
    if not (shell / "Commons").is_dir():
        sys.exit(f"no shell modules at {shell}")
    engine_binary = Path(args.engine).resolve()
    if not engine_binary.is_file():
        sys.exit(f"build the engine first: {engine_binary} is missing")

    work = Path(args.work).resolve()
    work.mkdir(parents=True, exist_ok=True)
    home, prefix, runtime, wrapper = build_world(work, omarchy, args.theme, engine_binary)
    seed_theme(wrapper, "matte", "matte-black", "Maarten I")
    imports = build_imports(work, shell)

    env = {
        "HOME": str(home),
        "XDG_RUNTIME_DIR": str(runtime),
        "OMARCHY_PATH": str(omarchy),
        "XDG_CONFIG_HOME": str(home / ".config"),
        "XDG_STATE_HOME": str(home / ".local/state"),
    }
    Process.env = env

    QQuickWindow.setGraphicsApi(QSGRendererInterface.GraphicsApi.Software)
    qInstallMessageHandler(lambda mode, ctx, msg: print("qt:", msg, file=sys.stderr))
    qt_app = QGuiApplication(sys.argv)

    quickshell = HarnessEnv(env)  # kept alive for the engine's lifetime
    qmlRegisterSingletonInstance(HarnessEnv, "Quickshell", 1, 0, "Quickshell", quickshell)
    qmlRegisterType(QUrl.fromLocalFile(str(HERE / "FloatingWindow.qml")), "Quickshell", 1, 0, "FloatingWindow")
    qmlRegisterType(QUrl.fromLocalFile(str(HERE / "ShellRoot.qml")), "Quickshell", 1, 0, "ShellRoot")
    qmlRegisterType(Process, "Quickshell.Io", 1, 0, "Process")
    qmlRegisterType(SplitParser, "Quickshell.Io", 1, 0, "SplitParser")
    qmlRegisterType(StdioCollector, "Quickshell.Io", 1, 0, "StdioCollector")
    qmlRegisterType(FileView, "Quickshell.Io", 1, 0, "FileView")

    engine = QQmlApplicationEngine()
    engine.addImportPath(str(imports))
    warnings = []
    engine.warnings.connect(lambda errs: warnings.extend(str(e.toString()) for e in errs))
    engine.load(QUrl.fromLocalFile(str(Path(args.entry).resolve())))
    roots = engine.rootObjects()
    if not roots:
        for w in warnings:
            print(w, file=sys.stderr)
        sys.exit("the window did not load")
    root = roots[0]
    # The component's own context, so ids in Omaboot.qml resolve in scripts.
    context = QQmlEngine.contextForObject(root)

    def run(js):
        expr = QQmlExpression(context, root, js)
        value = expr.evaluate()
        if expr.hasError():
            print("script error:", expr.error().toString(), file=sys.stderr)
        # PySide hands back (value, isUndefined).
        if isinstance(value, tuple):
            value = value[0]
        return value

    def pump(seconds):
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            qt_app.processEvents()
            time.sleep(0.01)

    width, height = (int(v) for v in args.size.split("x"))
    windows = [w for w in root.findChildren(QQuickWindow)]
    if not windows:
        sys.exit("no window inside the root item")
    window = windows[0]
    window.setProperty("implicitWidth", width)
    window.setProperty("implicitHeight", height)
    # The plugin is opened by the harness; a generated app opens itself.
    is_plugin = Path(args.entry).name == "Omaboot.qml"
    app = "" if is_plugin else args.app + "."
    if is_plugin:
        run('open("{}")')
    pump(0.2)
    window.contentItem().setSize(window.size())

    def settle(until, timeout):
        end = time.monotonic() + timeout
        while time.monotonic() < end:
            pump(0.05)
            if run(until):
                return True
        return False

    if not settle(f"{app}info !== null && {app}busy === ''", args.timeout):
        print("timed out waiting for the engine; status:", run(f"{app}status"), file=sys.stderr)

    if args.select:
        run(f'{app}select("{args.select}")')
    if args.screen:
        run(f'{app}screen = "{args.screen}"')
    # Each script runs after the previous one has been saved and drawn, so
    # a sequence of edits behaves as a person clicking them one by one.
    idle = (f"{app}busy === '' && !{app}saving && ({app}selection === 'now' || {app}theme !== null)"
            f" && {app}previews[{app}screen] !== undefined")
    for js in args.script:
        settle(idle, args.timeout)
        run(js)
        pump(0.1 + args.script_pause)
        settle(idle, args.timeout)
    settle(f"{app}busy === '' && {app}previews[{app}screen] !== undefined", args.timeout)
    pump(args.settle)
    if args.expect_status is not None:
        status = str(run(f"{app}status"))
        if args.expect_status not in status:
            sys.exit(f"status is {status!r}, expected it to contain {args.expect_status!r}")

    image = window.grabWindow()
    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    if not image.save(str(out)):
        sys.exit(f"could not write {out}")
    print(f"wrote {out} ({image.width()}x{image.height()}); status: {run(app + 'status')}")
    for w in warnings:
        if "Quickshell" in w or "qs." in w:
            continue
        print("warning:", w, file=sys.stderr)
    if is_plugin:
        run("close()")
    qt_app.processEvents()


if __name__ == "__main__":
    main()
