#!/usr/bin/env python3
"""Check actual Slint switches and paths against the original v5 CSS/canvas.

--browser-reference also samples the owned Chromium tab displaying the original
HTML. It requires agent-browser and AGENT_BROWSER_CDP; it does not navigate tabs.
"""
import argparse
import base64
import json
import math
import os
import pathlib
import re
import subprocess

ROOT = pathlib.Path(__file__).resolve().parents[1]
BOUNCE = (.35, 1.7, .5, 1.)
EASE = (.25, .1, .25, 1.)


def bezier(time, curve):
    time = min(1., max(0., time))
    lo, hi = 0., 1.
    x1, y1, x2, y2 = curve
    for _ in range(60):
        u = (lo + hi) / 2
        x = 3*(1-u)**2*u*x1 + 3*(1-u)*u*u*x2 + u**3
        if x < time:
            lo = u
        else:
            hi = u
    return 3*(1-u)**2*u*y1 + 3*(1-u)*u*u*y2 + u**3


def css_samples(samples, curve=BOUNCE):
    phase, start, span, factor, origin, goal = "", 0., 0., 1., 1.5, 1.5
    result = []
    for s in samples:
        if s["phase"] == "hover":
            t = s["ms"]
            if t < 135:
                f = bezier(t/135, EASE)
                angle, y = -8*f, -.6*f
            elif t < 270:
                f = bezier((t-135)/135, EASE)
                angle, y = -8+13*f, -.6+.6*f
            elif t < 450:
                angle, y = 5-5*bezier((t-270)/180, EASE), 0.
            else:
                angle, y = 0., 0.
            result.append(dict(phase=s["phase"], ms=t, x=1.5, angle=angle, y=y))
            continue
        if phase != s["phase"]:
            next_start = s["clock"] - s["ms"]
            p = (next_start-start)/span if span else 1.
            eased = bezier(p, curve)
            current = origin + (goal-origin)*eased
            # CSS Transitions §3, reversing shortening factor.
            factor = min(1., abs(eased*factor+1-factor)) if p < 1 else 1.
            origin, goal = current, 25.5 if s["checked"] else 1.5
            phase, start, span = s["phase"], next_start, 500*factor
        progress = bezier(s["ms"]/span if span else 1., curve)
        result.append(dict(phase=phase, ms=s["ms"], x=origin+(goal-origin)*progress, angle=0., y=0.))
    return result


def browser_samples(samples):
    script = "(()=>{const trace=" + json.dumps(samples) + ";return " + r"""(()=>{
const row=document.querySelector('#p-st .srow').cloneNode(true),kn=row.querySelector('.kn'),track=row.querySelector('.catsw');
row.removeAttribute('id');row.setAttribute('aria-checked','false');
row.style.cssText='position:absolute;left:0;top:-1000px;width:500px;pointer-events:none';
kn.style.animation='none';document.querySelector('#app').append(row);
const result=[],phases=[];let phase='',start=0,transition=null,hover=null,background=null;
try{
getComputedStyle(kn).left;
for(const s of trace){
 if(s.phase!==phase){
  const nextStart=s.clock-s.ms;
  if(transition){transition.currentTime=nextStart-start;getComputedStyle(kn).left;}
  if(background){background.currentTime=nextStart-start;getComputedStyle(track).backgroundColor;}
  phase=s.phase;start=nextStart;
  if(phase==='hover'){
   kn.style.animation='twitch .45s ease';getComputedStyle(kn).transform;
   hover=kn.getAnimations().find(a=>a.animationName==='twitch');hover.pause();
  }else{
   row.setAttribute('aria-checked',String(s.checked));getComputedStyle(kn).left;
   transition=kn.getAnimations().find(a=>a.transitionProperty==='left');
   if(transition)transition.pause();
   getComputedStyle(track).backgroundColor;
   background=track.getAnimations().find(a=>a.transitionProperty==='background-color');
   if(background)background.pause();
  }
  phases.push({phase,duration:transition?.effect.getComputedTiming().duration??null});
 }
 if(phase==='hover')hover.currentTime=s.ms;else if(transition)transition.currentTime=s.ms;
 if(background)background.currentTime=s.ms;
 const c=getComputedStyle(kn),m=new DOMMatrix(c.transform==='none'?undefined:c.transform);
 const color=getComputedStyle(track).backgroundColor,parts=color.match(/[\d.]+/g).map(Number),fill=parts.length>3?parts[3]:1;
 result.push({phase,ms:s.ms,x:parseFloat(c.left)+1.5,angle:Math.atan2(m.b,m.a)*180/Math.PI,y:m.d?m.f/m.d:0,fill});
}
return {phases,samples:result};
}finally{row.remove();}
})()})()
"""
    process = subprocess.run(
        ["agent-browser", "--cdp", os.environ["AGENT_BROWSER_CDP"], "--json", "eval", "--stdin"],
        input=script, text=True, capture_output=True, timeout=25)
    assert process.returncode == 0, process.stdout + process.stderr
    data = json.loads(process.stdout)
    assert data["success"], data
    return data["data"]["result"]


def points(path):
    return [tuple(map(float, v)) for v in re.findall(r"[ML]\s*([-\d.]+)\s+([-\d.]+)", path)]


def rendered_frames(frames):
    poses = [dict(angle=f["angle"], y=f["y"], scale=f["scale"]) for f in frames]
    script = "(async()=>{const poses=" + json.dumps(poses) + ";" + r"""
const source=document.querySelector('#p-st .srow .kn').innerHTML,results=[];
const encode=bytes=>{let text='';for(let i=0;i<bytes.length;i+=8192)text+=String.fromCharCode(...bytes.subarray(i,i+8192));return btoa(text)};
for(const pose of poses){
 const size=Math.round(28*pose.scale),canvas=document.createElement('canvas');
 canvas.width=size;canvas.height=size;
 const context=canvas.getContext('2d');context.fillStyle='#FFFFFF';context.fillRect(0,0,size,size);
 const style='.h{fill:#E7E5ED;stroke:#1C1926;stroke-width:1.75;stroke-linejoin:round}.e-s{fill:none;stroke:#1C1926;stroke-width:1.75;stroke-linecap:round}.e-o{display:none}';
 const svg='<svg xmlns="http://www.w3.org/2000/svg" width="'+size+'" height="'+size+'" viewBox="0 0 28 28"><style>'+style+'</style><g transform="rotate('+pose.angle+' 14 14) translate(0 '+pose.y+')"><svg width="28" height="28" viewBox="0 0 24 24">'+source+'</svg></g></svg>';
 const image=new Image();image.src='data:image/svg+xml;base64,'+btoa(svg);await image.decode();
 context.drawImage(image,0,0);const rgba=context.getImageData(0,0,size,size).data;
 results.push({size,rgba:encode(rgba)});
}
const scale=poses[0].scale,canvas=document.createElement('canvas');canvas.width=1200*scale;canvas.height=134*scale;
const context=canvas.getContext('2d');context.setTransform(scale,0,0,scale,0,0);
const savedL=L,savedS={...S},savedTw=tw;
try{
 L={x:context,w:1200,h:134};Object.assign(S,{page:2,on:true,err:'sub',mode:'rule',sys:true,tun:false,load:0,hold:0,holding:false,ex:500,ev:0});tw=0;drawLine(0);
}finally{L=savedL;Object.assign(S,savedS);tw=savedTw}
context.setTransform(1,0,0,1,0,0);context.globalCompositeOperation='destination-over';context.fillStyle='#FFFFFF';context.fillRect(0,0,canvas.width,canvas.height);
return {frames:results,wave:{width:canvas.width,height:54*scale,rgba:encode(context.getImageData(0,25*scale,canvas.width,54*scale).data)}};
})()
"""
    process = subprocess.run(["agent-browser", "--cdp", os.environ["AGENT_BROWSER_CDP"],
                              "--json", "eval", "--stdin"], input=script, text=True,
                             capture_output=True, timeout=25)
    assert process.returncode == 0, process.stdout + process.stderr
    result = json.loads(process.stdout)
    assert result["success"], result
    errors = []
    reference_data = result["data"]["result"]
    for frame, reference in zip(frames, reference_data["frames"]):
        raw = pathlib.Path(frame["path"]).read_bytes()
        split = raw.index(b"\n255\n")+5
        _, width, height, _ = raw[:split].split()
        width, height = int(width), int(height)
        rgb, size = raw[split:], reference["size"]
        x0, y0 = round(40*frame["scale"]), round(50*frame["scale"])
        assert len(rgb) == width*height*3
        native_mask = {(x, y) for y in range(size) for x in range(size)
                       if sum(rgb[((y+y0)*width+x+x0)*3:((y+y0)*width+x+x0)*3+3]) < 384}
        rgba = base64.b64decode(reference["rgba"])
        image = pathlib.Path(frame["path"])
        image.with_suffix(".reference.ppm").write_bytes(
            f"P6\n{size} {size}\n255\n".encode()+bytes(v for i,v in enumerate(rgba) if i%4!=3))
        reference_mask = {(x, y) for y in range(size) for x in range(size)
                          if sum(rgba[(y*size+x)*4:(y*size+x)*4+3]) < 384}
        assert native_mask and reference_mask
        distance = ink_distance(native_mask, reference_mask)
        errors.append(dict(angle=frame["angle"], y=frame["y"], max_ink_distance_px=distance))
        assert distance <= 1.5, errors
    frame, reference = frames[0], reference_data["wave"]
    raw = pathlib.Path(frame["path"]).read_bytes()
    split = raw.index(b"\n255\n")+5
    _, stride, _, _ = raw[:split].split()
    stride = int(stride)
    rgb, width, height = raw[split:], reference["width"], reference["height"]
    y0 = round(166*frame["scale"])
    native_mask = {(x,y) for y in range(height) for x in range(width)
                   if sum(rgb[((y+y0)*stride+x)*3:((y+y0)*stride+x)*3+3]) < 384}
    rgba = base64.b64decode(reference["rgba"])
    expected_mask = {(x,y) for y in range(height) for x in range(width)
                     if sum(rgba[(y*width+x)*4:(y*width+x)*4+3]) < 384}
    wave_error = ink_distance(native_mask, expected_mask)
    assert wave_error <= 1.5, ("actual error-state waveform raster differs", wave_error)
    return dict(switch_frames=errors, error_state_wave_max_ink_distance_px=wave_error)


def ink_distance(first, second):
    assert first and second
    offsets = [(x,y,math.hypot(x,y)) for y in range(-3,4) for x in range(-3,4)]
    def distance(mask, other):
        return max(0. if (x,y) in other else
                   min((d for dx,dy,d in offsets if (x+dx,y+dy) in other), default=math.inf)
                   for x,y in mask)
    return max(distance(first,second), distance(second,first))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=pathlib.Path, default=ROOT/"target/release/Purr")
    parser.add_argument("--report", type=pathlib.Path, default=ROOT/"ui-preview/switch-report.json")
    parser.add_argument("--browser-reference", action="store_true")
    args = parser.parse_args()
    binary, report = args.binary.resolve(), args.report.resolve()
    report.parent.mkdir(parents=True, exist_ok=True)
    native_path = report.with_suffix(".native.json")
    wave_path = report.with_suffix(".wave.json")
    reference = ROOT/"tools/fixtures/wave-v5.json"
    for options in [
        ["--switch-check", str(native_path)],
        ["--wave-check", str(reference), "--wave-output", str(wave_path)],
    ]:
        subprocess.run([str(binary), "--preview", *options], check=True, timeout=25,
                       stdout=subprocess.DEVNULL)
    native = json.loads(native_path.read_text())
    checks = {key: value for key, value in native.items() if key not in ["samples", "frames"]}
    assert all(checks.values()), checks
    samples = native["samples"]
    hover = [s for s in samples if s["phase"] == "hover"]
    assert min(s["y"] for s in hover) < -.55, "hover missed its translation peak"
    assert min(s["angle"] for s in hover) < -7 and max(s["angle"] for s in hover) > 4.8
    expected = css_samples(samples)
    for value, fill in zip(expected, css_samples(samples, EASE)):
        value["fill"] = (fill["x"]-1.5)/24
    browser = browser_samples(samples) if args.browser_reference else None
    rendered = rendered_frames(native["frames"]) if args.browser_reference else None
    errors = {}
    for name, reference_samples in [("css", expected), ("chromium", browser["samples"] if browser else None)]:
        if reference_samples is None:
            continue
        errors[name] = {key: max(abs(a[key]-b[key]) for a, b in zip(samples, reference_samples))
                        for key in ["x", "angle", "y", "fill"]}
        assert errors[name]["x"] < .01 and errors[name]["angle"] < .001 and errors[name]["y"] < .001, errors
        assert errors[name]["fill"] < (.003 if name == "chromium" else .00001), errors
    source, waves = json.loads(reference.read_text()), json.loads(wave_path.read_text())
    wave_errors, count = {}, 0
    for r, n in zip(source["cases"], waves["cases"]):
        assert r["input"]["name"] == n["name"]
        values = dict(points(n["solid"]["main"]))
        error = max(abs(values[x]-y) for x, y in r["points"])
        assert error < .002, (n["name"], error)
        wave_errors[n["name"]] = error
        count += len(r["points"])
        assert points(n["solid"]["main"])[-1][0] == r["input"]["width"]
        if n["name"] == "proxy-off":
            segments = re.findall(r"M[^M]+", n["rendered"]["main"])
            lengths = [sum(math.dist(a, b) for a, b in zip(points(s), points(s)[1:])) for s in segments]
            assert len(segments) > 50 and max(abs(v-5) for v in lengths[:-1]) < .003
        if n["name"] == "hold":
            assert points(n["solid"]["warm"])[-1] == points(n["solid"]["main"])[0]
    result = dict(state_checks=checks, switch_samples=len(samples), switch_max_errors=errors,
                  wave_samples=count, wave_max_errors=wave_errors,
                  chromium_phases=browser["phases"] if browser else None,
                  rendered_switch_frames=rendered,
                  limits="Switch frames and sampled timing/geometry; not full app pixel equivalence.")
    report.write_text(json.dumps(result, indent=2)+"\n")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
