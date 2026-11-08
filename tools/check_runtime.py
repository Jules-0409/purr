#!/usr/bin/env python3
"""Exercise the real GUI + Mihomo without changing system proxy or using external traffic."""
import argparse, concurrent.futures, ctypes, http.client, http.server, json, os
import pathlib, signal, socket, subprocess, tempfile, threading, time, urllib.request
ROOT=pathlib.Path(__file__).resolve().parents[1]

def footprint(pid):
    if not pid or os.name=="nt" or os.uname().sysname!="Darwin":return None
    # Darwin rusage_info_v0, defined by the SDK's sys/resource.h.
    class Usage(ctypes.Structure):
        _fields_=[("uuid",ctypes.c_ubyte*16)]+[(n,ctypes.c_uint64) for n in [
            "user","system","package_wakeups","interrupt_wakeups","pageins",
            "wired","resident","physical","start","exit"]]
    data=Usage()
    lib=ctypes.CDLL("/usr/lib/libproc.dylib")
    if lib.proc_pid_rusage(pid,0,ctypes.byref(data))==0:return data.physical
    return None

def sample(shell,core):
    result={"shell":rss(shell),"core":rss(core) if core else 0}
    if os.name!="nt" and os.uname().sysname=="Darwin":
        result.update(shell_physical=footprint(shell) or 0,core_physical=footprint(core) or 0)
    return result

def rss(pid):
    if os.name=="nt":
        class Counters(ctypes.Structure):
            _fields_=[("cb",ctypes.c_ulong),("faults",ctypes.c_ulong)]+[(n,ctypes.c_size_t) for n in ["peak","working","peakpaged","paged","peaknonpaged","nonpaged","pagefile","peakpagefile"]]
        k=ctypes.windll.kernel32
        k.OpenProcess.restype=ctypes.c_void_p
        h=k.OpenProcess(0x1010,False,pid)
        if not h:return 0
        c=Counters();c.cb=ctypes.sizeof(c)
        k.K32GetProcessMemoryInfo(ctypes.c_void_p(h),ctypes.byref(c),c.cb);k.CloseHandle(ctypes.c_void_p(h))
        return c.working
    if os.uname().sysname=="Darwin":
        lib=ctypes.CDLL("/usr/lib/libproc.dylib")
        data=ctypes.create_string_buffer(96)
        if lib.proc_pidinfo(pid,4,0,data,96)>0:
            return ctypes.c_uint64.from_buffer(data,8).value
        return 0
    for line in pathlib.Path(f"/proc/{pid}/status").read_text().splitlines():
        if line.startswith("VmRSS:"):return int(line.split()[1])*1024
    return 0

def main():
    p=argparse.ArgumentParser()
    p.add_argument("--binary",type=pathlib.Path,default=ROOT/"target/release/Purr")
    p.add_argument("--report",type=pathlib.Path,default=ROOT/"ui-preview/runtime-report.json")
    p.add_argument("--memory-map",type=pathlib.Path)
    args=p.parse_args()
    args.binary=args.binary.resolve()
    args.report.parent.mkdir(parents=True,exist_ok=True)
    (ROOT/"ui-preview").mkdir(exist_ok=True)
    def port():
        with socket.socket() as s:s.bind(("127.0.0.1",0));return s.getsockname()[1]
    controller,proxy=port(),port()
    secret="purr-local-validation"
    provider={"alternate_title":"Fixture B"}
    class Handler(http.server.BaseHTTPRequestHandler):
        def log_message(self,*a):pass
        def do_GET(self):
            if self.path in ["/valid-profile","/alternate-profile","/expired-profile","/invalid-profile","/edited-profile","/edited-alternate-profile"]:
                name={"/valid-profile":"Fixture A","/alternate-profile":provider["alternate_title"],"/expired-profile":"Expired fixture","/invalid-profile":"Invalid fixture","/edited-profile":"Fixture A","/edited-alternate-profile":"Fixture B edited"}[self.path]
                payload=(config.replace("Test group","Alternate group") if self.path in ["/alternate-profile","/edited-alternate-profile"] else config).encode() if self.path!="/invalid-profile" else b"invalid YAML: ["
                self.send_response(200);self.send_header("Content-Length",str(len(payload)))
                self.send_header("profile-title",name)
                self.send_header("subscription-userinfo",f"upload=1073741824; download=2147483648; total=10737418240; expire={1 if self.path=='/expired-profile' else int(time.time())+86400*365}")
                self.end_headers()
                self.wfile.write(payload);return
            payload=b"purr-check\n"*4096
            self.send_response(200);self.send_header("Content-Length",str(len(payload)*64));self.end_headers()
            try:
                for _ in range(64):self.wfile.write(payload);self.wfile.flush();time.sleep(.015)
            except (BrokenPipeError,ConnectionResetError):pass
    server=http.server.ThreadingHTTPServer(("127.0.0.1",0),Handler)
    threading.Thread(target=server.serve_forever,daemon=True).start()
    def api(route,method="GET",body=None):
        data=json.dumps(body).encode() if body is not None else None
        req=urllib.request.Request(f"http://127.0.0.1:{controller}{route}",data=data,method=method,headers={"Authorization":f"Bearer {secret}","Content-Type":"application/json"})
        with urllib.request.build_opener(urllib.request.ProxyHandler({})).open(req,timeout=6) as response:
            data=response.read()
            return json.loads(data) if data else None
    samples=[];checks=[];core=0;complete=False
    def system_proxy_state():
        if os.name=="nt":
            import winreg
            values={}
            with winreg.OpenKey(winreg.HKEY_CURRENT_USER,r"Software\Microsoft\Windows\CurrentVersion\Internet Settings") as key:
                for name in ["ProxyEnable","ProxyServer","ProxyOverride","AutoConfigURL","AutoDetect"]:
                    try: values[name]=winreg.QueryValueEx(key,name)
                    except FileNotFoundError: pass
            return values
        if os.uname().sysname=="Darwin":
            return subprocess.check_output(["/usr/sbin/scutil","--proxy"])
        return None
    baseline=system_proxy_state()
    with tempfile.TemporaryDirectory(prefix="purr-runtime-") as temp:
        directory=pathlib.Path(temp)
        config=f'mixed-port: {proxy}\nexternal-controller: "127.0.0.1:{controller}"\nsecret: "{secret}"\nmode: rule\nlog-level: warning\nallow-lan: false\nbind-address: 127.0.0.1\nipv6: true\nproxies: []\nproxy-groups:\n  - name: Test group\n    type: select\n    proxies: [DIRECT, REJECT]\nrules:\n'
        config+="".join(f"  - DOMAIN-SUFFIX,validation-{i}.invalid,DIRECT\n" for i in range(2000))
        config+="  - MATCH,DIRECT\n"
        (directory/"config.yaml").write_text(config)
        (directory/"purr-validation-fixture").touch()
        # Fixture runs are pre-accepted so the first-launch notice does not block the harness.
        (directory / "disclaimer.json").write_text(__import__("json").dumps({"disclaimer_accepted": True, "version": 1, "accepted_at": int(__import__("time").time())}))
        (directory/"preferences.json").write_text(json.dumps({"mixed_port":proxy,"auto_wake":False,"system_proxy":False}))
        env=dict(os.environ,PURR_CONFIG_DIR=str(directory),PURR_CONTROLLER_ADDR=f"127.0.0.1:{controller}")
        log=(directory/"run.log").open("w")
        process=subprocess.Popen([str(args.binary),"--runtime-ui-check"],env=env,stdout=log,stderr=subprocess.STDOUT)
        started=time.monotonic()
        try:
            for _ in range(120):
                samples.append(sample(process.pid,core))
                text=(directory/"run.log").read_text()
                for line in text.splitlines():
                    if "mihomo pid " in line:
                        core=int(line.rsplit(" ",1)[1])
                try:
                    if api("/version"):break
                except Exception:pass
                time.sleep(.1)
            else:raise AssertionError("Controller failed to start; see isolated runtime log")
            assert len(api("/rules")["rules"])==2001
            checks.append("fresh profile + 2,001 real rules loaded")
            for mode in ["global","direct","rule"]:
                api("/configs","PATCH",{"mode":mode})
                assert api("/configs")["mode"]==mode
            checks.append("all three outbound modes")
            api("/proxies/Test%20group","PUT",{"name":"REJECT"})
            assert api("/proxies")["proxies"]["Test group"]["now"]=="REJECT"
            api("/proxies/Test%20group","PUT",{"name":"DIRECT"})
            checks.append("node selection persisted in controller")
            def transfer(_):
                c=http.client.HTTPConnection("127.0.0.1",proxy,timeout=15)
                c.request("GET",f"http://127.0.0.1:{server.server_port}/data")
                response=c.getresponse()
                assert response.status==200
                count=0
                while chunk:=response.read(65536):count+=len(chunk)
                c.close()
                return count
            with concurrent.futures.ThreadPoolExecutor(max_workers=16) as pool:
                jobs=[pool.submit(transfer,i) for i in range(64)]
                saw_connections=False
                while not all(j.done() for j in jobs):
                    samples.append(sample(process.pid,core))
                    saw_connections|=len(api("/connections").get("connections") or [])>0
                    time.sleep(.1)
                transferred=sum(j.result() for j in jobs)
            assert saw_connections
            checks.append("64 real proxy transfers, 16 concurrent")
            def cancellable_transfer():
                try:return transfer(0)
                except (OSError,http.client.HTTPException):return 0
            def wait_connections(count):
                for _ in range(100):
                    connections=api("/connections")["connections"] or []
                    if len(connections)>=count:return connections
                    time.sleep(.01)
                raise AssertionError("Expected live proxy connections before disconnect")
            with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
                job=pool.submit(cancellable_transfer)
                connection=wait_connections(1)[0]["id"]
                api(f"/connections/{connection}","DELETE")
                assert all(c["id"]!=connection for c in (api("/connections")["connections"] or []))
                assert job.result()<transferred//64
                checks.append("disconnect one active transfer by connection ID")
                jobs=[pool.submit(cancellable_transfer) for _ in range(2)]
                wait_connections(2)
                api("/connections","DELETE")
                assert not api("/connections")["connections"]
                assert all(j.result()<transferred//64 for j in jobs)
                checks.append("disconnect all active transfers")
            sequence=0
            def ui_action(action,value,condition,timeout=18):
                nonlocal sequence
                sequence+=1
                temporary=directory/"ui-request.tmp"
                temporary.write_text(json.dumps({"id":sequence,"action":action,"value":str(value)}))
                temporary.replace(directory/"ui-request.json")
                deadline=time.monotonic()+timeout;state={}
                while time.monotonic()<deadline:
                    assert process.poll() is None,"GUI exited while driving real UI callbacks"
                    try:
                        accepted=json.loads((directory/"ui-accepted.json").read_text())
                        state=json.loads((directory/"ui-snapshot.json").read_text())
                        if accepted==sequence and condition(state):return state
                    except (FileNotFoundError,json.JSONDecodeError,KeyError):pass
                    samples.append(sample(process.pid,core))
                    time.sleep(.1)
                raise AssertionError(f"Real UI {action} did not reach expected state: {state}")
            def active(name):
                return lambda s:not s["busy"] and any(r["name"]==name and r["active"] for r in s["subscriptions"])
            ui_action("rule-query","validation-19.invalid",lambda s:s["rules"]==1)
            ui_action("rule-query","no-such-rule.invalid",lambda s:s["rules"]==0)
            ui_action("rule-query","",lambda s:s["rules"]==2001)
            checks.append("rule query updates and clears after cached status refreshes")
            base=f"http://127.0.0.1:{server.server_port}"
            a=ui_action("import",base+"/valid-profile",lambda s:active("Fixture A")(s) and s["rules"]==2005)
            assert len(a["subscriptions"])==1 and a["subscriptions"][0]["used"]=="3.0" and a["subscriptions"][0]["total"]=="10"
            assert a["rules"]==2005 and not a["first"]
            b=ui_action("import",base+"/alternate-profile",active("Fixture B"))
            assert len(b["subscriptions"])==2 and "Alternate group" in api("/proxies")["proxies"]
            ids={r["name"]:r["id"] for r in b["subscriptions"]}
            ui_action("use",ids["Fixture A"],active("Fixture A"))
            assert "Test group" in api("/proxies")["proxies"]
            provider["alternate_title"]="Fixture B updated"
            updated=ui_action("update",ids["Fixture B"],lambda s:active("Fixture A")(s) and any(r["name"]=="Fixture B updated" and not r["active"] for r in s["subscriptions"]))
            assert len(updated["subscriptions"])==2 and "Test group" in api("/proxies")["proxies"]
            before=(directory/"config.yaml").read_bytes()
            ui_action("import",base+"/invalid-profile",lambda s:not s["busy"] and "配置" in s["toast"])
            assert before==(directory/"config.yaml").read_bytes()
            assert len(json.loads((directory/"preferences.json").read_text())["subscriptions"])==2
            checks.append("native import clicks + two subscriptions + provider title/usage/expiry + cached switching + inactive update + rejected import rollback")
            cached=(directory/"subscriptions"/f'{ids["Fixture B"]}.yaml').read_bytes()
            ui_action("edit",ids["Fixture B"],lambda s:s["editing"]==ids["Fixture B"])
            ui_action("import",base+"/invalid-profile",lambda s:not s["busy"] and "配置" in s["toast"])
            assert before==(directory/"config.yaml").read_bytes()
            assert cached==(directory/"subscriptions"/f'{ids["Fixture B"]}.yaml').read_bytes()
            edited=ui_action("import",base+"/edited-alternate-profile",lambda s:active("Fixture A")(s) and s["editing"]=="" and any(r["name"]=="Fixture B edited" and r["id"]==ids["Fixture B"] for r in s["subscriptions"]))
            assert len(edited["subscriptions"])==2 and before==(directory/"config.yaml").read_bytes()
            ui_action("edit",ids["Fixture A"],lambda s:s["editing"]==ids["Fixture A"])
            edited=ui_action("import",base+"/edited-profile",lambda s:active("Fixture A")(s) and s["editing"]=="")
            assert len(edited["subscriptions"])==2
            prefs=json.loads((directory/"preferences.json").read_text())
            assert prefs["subscription"].endswith("/edited-profile")
            assert next(r for r in prefs["subscriptions"] if str(r["id"])==ids["Fixture A"])["url"]==prefs["subscription"]
            checks.append("active and inactive edits preserve subscription IDs and count; rejected edits preserve active config and cached profile")
            with socket.socket() as occupied:
                occupied.bind(("127.0.0.1",0));occupied.listen()
                blocked=occupied.getsockname()[1]
                ui_action("port",blocked,lambda s:s["trouble"]=="port")
                recovered=ui_action("fix","primary",lambda s:s["trouble"]=="" and int(s["port"]) not in [proxy,blocked])
                with socket.create_connection(("127.0.0.1",int(recovered["port"])),timeout=2):pass
            ui_action("port",proxy,lambda s:int(s["port"])==proxy)
            checks.append("occupied-port error rendered in real UI + recovery rebind + original listener restored")
            expired=ui_action("import",base+"/expired-profile",lambda s:active("Expired fixture")(s) and s["trouble"]=="sub")
            expired_id=next(r["id"] for r in expired["subscriptions"] if r["name"]=="Expired fixture")
            assert next(r for r in expired["subscriptions"] if r["id"]==expired_id)["expired"]
            ui_action("fix","secondary",lambda s:active("Fixture A")(s) and s["trouble"]=="")
            ui_action("use",expired_id,lambda s:active("Fixture A")(s) and "已到期" in s["toast"])
            checks.append("provider expiry triggers real UI trouble + backup recovery + expired cache cannot be selected")
            local=directory/"Local profile.yaml";local.write_text(config)
            local_state=ui_action("file",local,active("Local profile"))
            assert len(local_state["subscriptions"])==4
            before=(directory/"config.yaml").read_bytes()
            oversized=directory/"Oversized.yaml";oversized.write_bytes(b"#"*(2*1024*1024+1))
            ui_action("file",oversized,lambda s:active("Local profile")(s) and "超过" in s["toast"])
            assert before==(directory/"config.yaml").read_bytes()
            ui_action("use",ids["Fixture A"],active("Fixture A"))
            checks.append("local YAML import + file metadata + bounded oversized-file rejection + active config preserved")
            ui_action("ui-dns",1,lambda s:s["dns"]=="8.8.8.8")
            assert json.loads((directory/"preferences.json").read_text())["dns"]=="8.8.8.8"
            ui_action("ui-dns",0,lambda s:s["dns"]=="1.1.1.1")
            ui_action("ui-switch",4,lambda s:not s["ipv6"])
            ui_action("ui-switch",4,lambda s:s["ipv6"])
            burst=ui_action("ui-switch-burst","",lambda s:s["pending_settings"]==0 and not s["ipv6"])
            assert not burst["ui_ipv6"] and not api("/configs")["ipv6"]
            assert not json.loads((directory/"preferences.json").read_text())["ipv6"]
            ui_action("ui-switch-burst","",lambda s:s["pending_settings"]==0 and s["ipv6"])
            checks.append("seven rapid switch clicks preserve order and agree in UI, core and persisted settings")
            preferences=directory/"preferences.json"
            backup=directory/"preferences.fixture-backup"
            before_config=(directory/"config.yaml").read_bytes()
            preferences.rename(backup)
            preferences.mkdir()
            try:
                failed=ui_action("ui-switch",4,lambda s:s["pending_settings"]==0 and s["ipv6"] and s["ui_ipv6"])
                assert failed["toast"] and api("/configs")["ipv6"]
                assert (directory/"config.yaml").read_bytes()==before_config
            finally:
                preferences.rmdir()
                backup.rename(preferences)
                preferences.with_suffix(".purr-tmp").unlink(missing_ok=True)
            checks.append("failed preference save rolls back the visible switch, live state, core and profile")
            ui_action("ui-sound",1,lambda s:s["sound"]=="ping")
            ui_action("ui-sound",0,lambda s:s["sound"]=="off")
            checks.append("supplied settings controls persist DNS, IPv6 and sound through real command handlers")
            ui_action("native-tray","",lambda s:s["native_tray"])
            ui_action("native-tray-mode","",lambda s:s["mode"]=="global")
            ui_action("native-tray-focus-main","",lambda s:not s["native_tray"])
            ui_action("native-tray","",lambda s:s["native_tray"])
            ui_action("native-tray-escape","",lambda s:not s["native_tray"])
            ui_action("native-tray","",lambda s:s["native_tray"])
            ui_action("native-tray","",lambda s:not s["native_tray"])
            checks.append("native OS popover opens, handles a pointer mode selection, dismisses on focus loss/Escape and toggles closed")
            ui_action("close","",lambda s:not s["open"])
            reopened=ui_action("open","",lambda s:s["open"] and len(s["subscriptions"])==4 and s["rules"]>=2001)
            assert any(r["active"] and r["name"]=="Fixture A" for r in reopened["subscriptions"])
            checks.append("close to tray + reopen rebuilds all subscription and rule models")
            for page in range(6):
                ui_action("page",page,lambda s,p=page:s["page"]==p)
            if args.memory_map and os.name!="nt" and os.uname().sysname=="Darwin":
                with args.memory_map.open("w") as output:
                    subprocess.run(["/usr/bin/vmmap","-summary",str(process.pid)],stdout=output,stderr=subprocess.STDOUT,check=True)
            verifier=subprocess.Popen([str(args.binary),"--verify-backend",f"http://127.0.0.1:{server.server_port}"],env=env)
            limit=time.monotonic()+90
            while verifier.poll() is None:
                samples.append(sample(process.pid,core))
                if time.monotonic()>limit:verifier.kill();verifier.wait();raise AssertionError("Backend verifier timed out")
                time.sleep(.05)
            assert verifier.returncode==0,"Backend settings verification failed"
            checks.append("real settings + DNS + ad rules + port rebinding + subscription import + failed-edit preservation")
            # Recovery is exercised by killing only the supervised test core.
            os.kill(core,signal.SIGTERM)
            previous=core
            for _ in range(90):
                text=(directory/"run.log").read_text()
                pids=[int(l.rsplit(" ",1)[1]) for l in text.splitlines() if "mihomo pid " in l]
                core=pids[-1] if pids else core
                if core!=previous:
                    try:
                        if api("/version"):break
                    except Exception:pass
                time.sleep(.1)
            else:raise AssertionError("Supervised core did not recover")
            checks.append("core exit + supervised restart")
            while time.monotonic()-started<12:
                samples.append(sample(process.pid,core))
                time.sleep(.1)
            text=(directory/"run.log").read_text()
            assert all(f"switch page to {i}" in text for i in range(6)),"Native runtime did not visit all six pages"
            checks.append("all six real UI pages selected via native pointer events")
            samples.append(sample(process.pid,core))
            complete=True
        finally:
            if os.name=="nt":
                subprocess.run(["taskkill","/PID",str(process.pid),"/T","/F"],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
            else:process.terminate()
            try:process.wait(timeout=12)
            except subprocess.TimeoutExpired:process.kill();process.wait()
            log.close();server.shutdown()
            if baseline is not None:
                assert baseline==system_proxy_state(),"System proxy unexpectedly changed"
            # The process log contains only the local synthetic fixture, never a user profile.
            (ROOT/"ui-preview/runtime-sandbox.log").write_text((directory/"run.log").read_text())
        if not complete:raise AssertionError("Runtime checks incomplete")
    peak=max(s["shell"]+s["core"] for s in samples)
    assert max(s["shell"] for s in samples)>1_000_000,"GUI memory measurement failed"
    assert max(s["core"] for s in samples)>1_000_000,"Core memory measurement failed"
    report={"checks":checks,"transferred_bytes":transferred,"samples":samples,"peak_combined_mb":peak/1e6,"shell_peak_mb":max(s["shell"] for s in samples)/1e6,"core_peak_mb":max(s["core"] for s in samples)/1e6,"budget_mb":150,"passed":peak<150_000_000,"system_proxy_unchanged":True if baseline is not None else None}
    report.update(platform=__import__("platform").platform(),renderer=os.environ.get("PURR_RENDERER","software"),
                  binary_sha256=__import__("hashlib").sha256(args.binary.read_bytes()).hexdigest(),
                  metric="combined RSS, decimal MB",scope="GUI and supervised core; temporary config-validator subprocesses excluded")
    if "shell_physical" in samples[0]:
        report["peak_combined_physical_mb"]=max(s["shell_physical"]+s["core_physical"] for s in samples)/1e6
    args.report.write_text(json.dumps(report,indent=2))
    print(json.dumps({k:v for k,v in report.items() if k!="samples"},indent=2))
    if not report["passed"]:raise SystemExit(2)
if __name__=="__main__":main()
