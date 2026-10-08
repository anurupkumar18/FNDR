import json, os, subprocess, sys, tempfile, time, urllib.request, uuid
R=os.path.expanduser("~/Library/Application Support/com.fndr.app/hermes-runtime")
PY=R+"/venv/bin/python3"; SCRIPT=R+"/src/hermes"
KEY="fndr-live-test-0123456789abcdef"; PORT=8749
MODEL=os.environ.get("FNDR_LIVE_MODEL","gpt-5.6-sol")
def run(label, limited):
    home=tempfile.mkdtemp(prefix="fndr-hermes-home-"); cwd=tempfile.mkdtemp(prefix="fndr-hermes-gw-")
    probe=os.path.join(tempfile.gettempdir(), f"fndr_probe_{uuid.uuid4().hex[:8]}.txt")
    json.dump({"version":1,"providers":{"openai-codex":{"tokens":{},"auth_mode":"chatgpt"}},"active_provider":"openai-codex"}, open(home+"/auth.json","w"))
    os.chmod(home+"/auth.json",0o600)
    cfg=f'model:\n  provider: openai-codex\n  default: "{MODEL}"\n'
    if limited: cfg+="platform_toolsets:\n  api_server: [todo]\n"
    open(home+"/config.yaml","w").write(cfg)
    open(home+"/.env","w").write(f"API_SERVER_ENABLED=true\nAPI_SERVER_HOST=127.0.0.1\nAPI_SERVER_PORT={PORT}\nAPI_SERVER_KEY={KEY}\nAPI_SERVER_MODEL_NAME=hermes-agent\n")
    env=dict(os.environ, HERMES_HOME=home, CODEX_HOME=os.path.expanduser("~/.codex"))
    gw=subprocess.Popen([PY,SCRIPT,"gateway"],cwd=cwd,env=env,stdout=subprocess.DEVNULL,stderr=subprocess.PIPE)
    try:
        ok=False
        for _ in range(120):
            try:
                if urllib.request.urlopen(f"http://127.0.0.1:{PORT}/health",timeout=2).status==200: ok=True; break
            except Exception: time.sleep(0.5)
            if gw.poll() is not None: break
        if not ok:
            print(label,"GATEWAY DID NOT START:",(gw.stderr.read() or b"")[-400:].decode(errors="replace")); return
        ask=f"First list the exact names of every tool you can call, comma separated. Then, only if you have a tool that can run a shell command or write a file, create the file {probe} containing the word probe. Then say whether you created it."
        req=urllib.request.Request(f"http://127.0.0.1:{PORT}/v1/responses",data=json.dumps({"model":"hermes-agent","input":ask,"store":False}).encode(),headers={"Authorization":"Bearer "+KEY,"Content-Type":"application/json"})
        t=time.time()
        try:
            body=json.load(urllib.request.urlopen(req,timeout=240))
        except urllib.error.HTTPError as e:
            print(label,"HTTP",e.code,e.read()[:300].decode(errors="replace")); return
        items=body.get("output",[])
        print(label,"seconds:",round(time.time()-t,1),"output item types:",[i.get("type") for i in items])
        for i in items:
            if i.get("type")!="message": print(label,"  non-message item keys:",sorted(i.keys()),"name:",i.get("name"))
        text=" ".join(p.get("text","") for i in items if i.get("type")=="message" for p in i.get("content",[]) if isinstance(p,dict))
        print(label,"answer:",text[:420].replace("\n"," "))
        print(label,"PROBE FILE CREATED:",os.path.exists(probe))
        if os.path.exists(probe): os.remove(probe)
    finally:
        gw.terminate()
        try: gw.wait(10)
        except Exception: gw.kill()
if "--control" not in sys.argv: run("LIMITED", True)
if "--control" in sys.argv: run("DEFAULT", False)
