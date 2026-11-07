#!/usr/bin/env python3
"""Legacy SVG comparison generator. Active components are in src/ui/native-icons.slint."""
import argparse, copy, json, pathlib, re, xml.etree.ElementTree as ET
ROOT=pathlib.Path(__file__).resolve().parents[1]
def main():
    p=argparse.ArgumentParser()
    p.add_argument("prototype",type=pathlib.Path)
    args=p.parse_args()
    icons=json.loads(re.search(r"const ICONS=(\{.*?\});",args.prototype.read_text(),re.S).group(1))
    out=ROOT/"src/ui/icons/motion";out.mkdir(exist_ok=True)
    def svg(name,nodes,offset=None):
        root=ET.Element("svg",{"xmlns":"http://www.w3.org/2000/svg","viewBox":"0 0 24 24","fill":"none","stroke":"#000","stroke-width":"1.75","stroke-linecap":"round","stroke-linejoin":"round"})
        for node in nodes:
            node=copy.deepcopy(node)
            for e in node.iter():
                if "f" in e.attrib.get("class","").split():e.set("fill","#000");e.set("stroke","none")
                if offset is not None:e.set("stroke-dasharray","2.6 2.2");e.set("stroke-dashoffset",str(offset))
            root.append(node)
        (out/f"{name}.svg").write_text(ET.tostring(root,encoding="unicode"))
    lines=['// Generated from original v5 SVG paths by tools/generate_icons.py.',
        'export global IconClock { in-out property <float> time: 0; in-out property <bool> awake: false; in-out property <bool> reduced: false; }',
        'export component CatIcon inherits Rectangle {',
        'in property <string> kind: "ov"; in property <bool> awake: IconClock.awake; in property <bool> hovered: false; in property <color> tint: #1C1926;',
        'width: 22px; height: 22px; private property <float> entered: 0; changed hovered => { root.entered = IconClock.time; }',
        'private property <float> ht: IconClock.time - root.entered;',
        'private property <float> blink: !root.awake || IconClock.reduced || mod(IconClock.time,4200) < 3948 ? 1 : max(0.1,abs(mod(IconClock.time,4200)-4074)/126);']
    for kind,markup in icons.items():
        nodes=list(ET.fromstring(f"<svg>{markup}</svg>"));base=[];parts={}
        for node in nodes:
            classes=node.get("class","").split()
            part=next((c for c in classes if c in {"head","ear","p2","thread","tail","peek","w1","w2","flow","e-o","e-s","sw","sz"}),None)
            (parts.setdefault(part,[]) if part else base).append(node)
        svg(f"{kind}-base",base)
        lines.append(f'if root.kind == "{kind}": Image {{ x: 0px; y: 0px; width: 100%; height: 100%; colorize: root.tint; source: @image-url("icons/motion/{kind}-base.svg"); }}')
        for part,nodes in parts.items():
            svg(f"{kind}-{part}",nodes)
            source=f'@image-url("icons/motion/{kind}-{part}.svg")'
            extra="";condition=f'root.kind == "{kind}"';height="100%";y="0px"
            if part in {"e-o","sw"}:condition+=" && root.awake"
            if part in {"e-s","sz"}:condition+=" && !root.awake"
            if part=="e-o":height="parent.height * root.blink";y="(parent.height - self.height) / 2"
            if part in {"head","ear"}:extra='transform-rotation: root.hovered && root.ht < 450 && !IconClock.reduced ? sin(root.ht / 450 * 720deg) * -8deg * (1 - root.ht / 450) : 0deg;'
            if part=="p2":y='root.hovered && root.ht < 500 && !IconClock.reduced ? -sin(root.ht / 500 * 180deg) * 1.5px : 0px'
            if part in {"thread","tail"}:extra='transform-rotation: (root.hovered || root.kind == "mouse") && !IconClock.reduced ? sin(IconClock.time / 1200 * 360deg) * 7deg - 7deg : 0deg;'
            if part=="peek":extra='opacity: root.hovered && root.ht < 900 && root.ht > 250 && root.ht < 650 && !IconClock.reduced ? 0 : 1;'
            if part in {"w1","w2"}:extra=f'opacity: root.awake && !IconClock.reduced ? 0.625 + cos((IconClock.time - {600 if part=="w2" else 0}) / 1800 * 360deg) * 0.375 : 1;'
            if part=="flow":
                for i in range(4):svg(f"{kind}-flow-{i}",nodes,-i*1.2)
                source='root.awake && !IconClock.reduced ? ('+' : '.join(f'mod(floor(IconClock.time / 225),4) == {i} ? @image-url("icons/motion/{kind}-flow-{i}.svg")' for i in range(3))+f' : @image-url("icons/motion/{kind}-flow-3.svg")) : {source}'
            lines.append(f'if {condition}: Image {{ x: 0px; y: {y}; width: 100%; height: {height}; colorize: root.tint; source: {source}; {extra} }}')
    lines.append("}")
    (ROOT/"src/ui/icons/motion/legacy-icon.slint").write_text("\n".join(lines)+"\n")
    print(f"Generated {len(icons)} layered icons")
if __name__=="__main__":main()
