#!/usr/bin/env python3
"""Real Mihomo probes and automatic failover through local HTTP proxy fixtures."""
import argparse
import http.server
import json
import os
import pathlib
import signal
import socket
import subprocess
import tempfile
import threading
import time
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parents[1]

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=pathlib.Path, default=ROOT/"target/release/Purr")
    parser.add_argument("--report", type=pathlib.Path, default=ROOT/"ui-preview/probes-report.json")
    args = parser.parse_args()
    state = {"fast": True, "counts": {"Fast": 0, "Slow": 0, "Dead": 0}}
    class Proxy(http.server.BaseHTTPRequestHandler):
        def log_message(self, *args): pass
        def do_CONNECT(self):
            # Mihomo's HTTP adapter tunnels the TCP stream, including a plain HTTP probe.
            name = self.server.label
            state["counts"][name] += 1
            self.close_connection = True
            if name == "Dead" or (name == "Fast" and not state["fast"]):
                return
            assert self.path.startswith("127.0.0.1:"), "Fixture must stay on loopback"
            self.wfile.write(b"HTTP/1.1 200 Connection established\r\n\r\n")
            self.wfile.flush()
            self.connection.settimeout(5)
            while self.rfile.readline().strip():
                pass
            time.sleep(0.02 if name == "Fast" else 0.2)
            self.wfile.write(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            self.wfile.flush()
        def do_GET(self):
            name = self.server.label
            state["counts"][name] += 1
            if name == "Dead" or (name == "Fast" and not state["fast"]):
                self.close_connection = True
                return
            time.sleep(0.02 if name == "Fast" else 0.2)
            self.send_response(204)
            self.send_header("Content-Length", "0")
            self.end_headers()
    servers = []
    for name in ["Fast", "Slow", "Dead"]:
        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Proxy)
        server.label = name
        servers.append(server)
        threading.Thread(target=server.serve_forever, daemon=True).start()
    def port():
        with socket.socket() as s:
            s.bind(("127.0.0.1", 0))
            return s.getsockname()[1]
    controller, mixed = port(), port()
    config = f'mixed-port: {mixed}\nexternal-controller: 127.0.0.1:{controller}\nsecret: probe-fixture\nmode: rule\nlog-level: warning\nproxies:\n'
    for server in servers:
        config += f"  - {{name: {server.label}, type: http, server: 127.0.0.1, port: {server.server_port}}}\n"
    config += "proxy-groups:\n  - name: Probe group\n    type: select\n    proxies: [Slow, Dead, Fast]\nrules: ['MATCH,DIRECT']\n"
    class Provider(http.server.BaseHTTPRequestHandler):
        def log_message(self, *args): pass
        def do_GET(self):
            data = config.encode()
            self.send_response(200)
            self.send_header("Content-Length", str(len(data)))
            self.send_header("profile-title", "Local probe fixture")
            self.end_headers()
            self.wfile.write(data)
    provider = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Provider)
    threading.Thread(target=provider.serve_forever, daemon=True).start()
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    def api(route):
        request = urllib.request.Request(f"http://127.0.0.1:{controller}{route}", headers={"Authorization": "Bearer probe-fixture"})
        with opener.open(request, timeout=4) as response: return json.load(response)
    report = {}
    with tempfile.TemporaryDirectory(prefix="purr-probes-") as temporary:
        directory = pathlib.Path(temporary)
        (directory/"purr-validation-fixture").touch()
        (directory/"config.yaml").write_text(config)
        (directory/"preferences.json").write_text(json.dumps({"mixed_port": mixed, "system_proxy": False, "auto_wake": False, "auto_swap": True}))
        env = dict(os.environ, PURR_CONFIG_DIR=str(directory), PURR_CONTROLLER_ADDR=f"127.0.0.1:{controller}", PURR_VALIDATION_PROBE_URL=f"http://127.0.0.1:{provider.server_port}/probe")
        log_path = args.report.with_name(args.report.stem + "-app.log")
        log_path.parent.mkdir(parents=True, exist_ok=True)
        with log_path.open("w") as log:
            process = subprocess.Popen([str(args.binary.resolve()), "--runtime-ui-check"], env=env, stdout=log, stderr=subprocess.STDOUT)
            try:
                def wait(condition, timeout=20):
                    end = time.monotonic()+timeout
                    while time.monotonic() < end:
                        assert process.poll() is None, "Native app exited"
                        try:
                            result = condition()
                            if result: return result
                        except (OSError, ValueError, KeyError): pass
                        time.sleep(0.1)
                    auto_swap = json.loads((directory/"preferences.json").read_text()).get("auto_swap")
                    raise AssertionError(f"Probe fixture did not reach expected state: counts={state['counts']}, auto_swap={auto_swap}, snapshot={snapshot()}, group={api('/proxies')['proxies'].get('Probe group')}")
                def snapshot(): return json.loads((directory/"ui-snapshot.json").read_text())
                sequence = 0
                def action(name, value=""):
                    nonlocal sequence
                    sequence += 1
                    pending = directory/"ui-request.tmp"
                    pending.write_text(json.dumps({"id": sequence, "action": name, "value": value}))
                    pending.replace(directory/"ui-request.json")
                    wait(lambda: json.loads((directory/"ui-accepted.json").read_text()) == sequence)
                wait(lambda: api("/version"))
                wait(lambda: snapshot()["open"])
                action("import", f"http://127.0.0.1:{provider.server_port}/profile")
                wait(lambda: snapshot()["subscriptions"] and not snapshot()["busy"])
                wait(lambda: api("/proxies")["proxies"]["Probe group"]["now"] == "Fast")
                assert all(state["counts"].values()), "All three real HTTP proxies must receive probes"
                report["automatic_import_probes_and_fastest_selection"] = True
                state["fast"] = False
                action("toggle-run")
                saw_timeout = False
                started = time.monotonic()
                def recovered():
                    nonlocal saw_timeout
                    saw_timeout |= snapshot()["trouble"] == "timeout"
                    return api("/proxies")["proxies"]["Probe group"]["now"] == "Slow"
                wait(recovered, 100)
                assert saw_timeout, "Timeout state and countdown must appear before failover"
                wait(lambda: snapshot()["trouble"] == "")
                report.update(three_failures_show_timeout_then_select_available_node=True, failover_seconds=round(time.monotonic()-started, 2), proxy_requests=state["counts"], passed=True,
                              binary_sha256=__import__("hashlib").sha256(args.binary.read_bytes()).hexdigest())
            finally:
                if os.name == "nt":
                    subprocess.run(["taskkill", "/PID", str(process.pid), "/T", "/F"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                else:
                    process.send_signal(signal.SIGTERM)
                try: process.wait(timeout=12)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
                for server in [*servers, provider]: server.shutdown()
    args.report.write_text(json.dumps(report, indent=2)+"\n")
    print(json.dumps(report, indent=2))

if __name__ == "__main__": main()
