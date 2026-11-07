#!/usr/bin/env python3
"""Verify actual macOS status-item clicks against an isolated native application."""
import argparse
import hashlib
import json
import os
import pathlib
import signal
import socket
import subprocess
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parents[1]

def main():
    p = argparse.ArgumentParser()
    p.add_argument("--binary", type=pathlib.Path, default=ROOT / "target/release/Purr")
    p.add_argument("--driver", type=pathlib.Path, default=ROOT / "ui-preview/macos-tray-click")
    p.add_argument("--report", type=pathlib.Path, default=ROOT / "ui-preview/macos-tray.json")
    args = p.parse_args()
    assert os.uname().sysname == "Darwin"
    def port():
        with socket.socket() as s:
            s.bind(("127.0.0.1", 0))
            return s.getsockname()[1]
    controller, proxy = port(), port()
    with tempfile.TemporaryDirectory(prefix="purr-native-tray-") as temp:
        directory = pathlib.Path(temp)
        (directory / "config.yaml").write_text(
            f'mixed-port: {proxy}\nexternal-controller: "127.0.0.1:{controller}"\n'
            'mode: rule\nlog-level: warning\nallow-lan: false\n'
            'proxies:\n  - name: Fixture node\n    type: http\n    server: 127.0.0.1\n    port: 9\n'
            'proxy-groups:\n  - name: Tray fixture\n    type: select\n'
            '    proxies: [Fixture node, DIRECT, REJECT]\nrules:\n  - MATCH,DIRECT\n')
        (directory / "purr-validation-fixture").touch()
        (directory / "preferences.json").write_text(json.dumps(
            {"mixed_port": proxy, "auto_wake": False, "system_proxy": False}))
        env = dict(os.environ, PURR_CONFIG_DIR=str(directory),
                   PURR_CONTROLLER_ADDR=f"127.0.0.1:{controller}")
        args.report.parent.mkdir(parents=True, exist_ok=True)
        log_path = args.report.with_name(args.report.stem + "-app.log")
        sequence = 0
        checks = []
        with log_path.open("w") as log:
            process = subprocess.Popen([str(args.binary.resolve()), "--runtime-ui-check"],
                                       env=env, stdout=log, stderr=subprocess.STDOUT)
            def state():
                try:
                    return json.loads((directory / "ui-snapshot.json").read_text())
                except (FileNotFoundError, json.JSONDecodeError):
                    return {}
            def wait(condition, label):
                deadline = time.monotonic() + 8
                while time.monotonic() < deadline:
                    assert process.poll() is None, "GUI exited"
                    current = state()
                    if condition(current):
                        return current
                    time.sleep(.05)
                raise AssertionError(f"{label}: {state()}")
            def action(name, value, condition):
                nonlocal sequence
                sequence += 1
                tmp = directory / "ui-request.tmp"
                tmp.write_text(json.dumps({"id": sequence, "action": name, "value": value}))
                tmp.replace(directory / "ui-request.json")
                def accepted(_):
                    try:
                        return json.loads((directory / "ui-accepted.json").read_text()) == sequence
                    except (FileNotFoundError, json.JSONDecodeError):
                        return False
                wait(accepted, f"{name} acknowledged")
                return wait(condition, name)
            def click(button, visible, hold=.05):
                subprocess.run([str(args.driver.resolve()), str(process.pid), button, str(hold)], check=True)
                wait(lambda s: s.get("native_tray") is visible, button)
                if visible:
                    time.sleep(.3)
                    assert state().get("native_tray") is True, "Popover immediately dismissed"
            try:
                wait(lambda s: s.get("open") is True, "initial window")
                action("page", "0", lambda s: s.get("page") == 0)
                click("left", True)
                checks.append("actual status-item left click opens and remains visible")
                code = (
                    'import ApplicationServices; '
                    'if let ws = CGWindowListCopyWindowInfo(.optionOnScreenOnly,kCGNullWindowID) as? [[String:Any]] { '
                    'for w in ws where (w[kCGWindowOwnerPID as String] as? Int) == '
                    f'{process.pid} && (w[kCGWindowName as String] as? String) == "PURR · 托盘" '
                    '{ print(w[kCGWindowNumber as String] as! Int) } }')
                window_id = int(subprocess.check_output(["/usr/bin/swift", "-e", code]).strip())
                subprocess.run(["/usr/sbin/screencapture", "-x", "-o", f"-l{window_id}",
                                str(args.report.with_suffix(".png"))], check=True)
                click("left", False)
                checks.append("repeat status-item left click closes")
                click("left", True)
                click("left", False, hold=1)
                checks.append("holding a closing status-item click does not reopen on release")
                click("right", True)
                checks.append("actual status-item right click opens designed popover")
                action("native-tray-escape", "", lambda s: not s.get("native_tray"))
                checks.append("Escape dismisses popover opened from status item")
                click("left", True)
                action("native-tray-focus-main", "", lambda s: not s.get("native_tray"))
                checks.append("main-window focus dismisses; subsequent status click reopens")
                click("right", True)
                action("native-tray-escape", "", lambda s: not s.get("native_tray"))
                args.report.write_text(json.dumps({"passed": True, "checks": checks,
                    "binary_sha256": hashlib.sha256(args.binary.read_bytes()).hexdigest()}, indent=2))
                print(json.dumps(checks, indent=2))
            finally:
                process.send_signal(signal.SIGTERM)
                try:
                    process.wait(timeout=12)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()

if __name__ == "__main__":
    main()
