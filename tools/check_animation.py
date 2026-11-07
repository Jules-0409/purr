#!/usr/bin/env python3
"""Compare actual Slint curve outputs with browser samples from the v5 HTML."""
import argparse
import json
import pathlib
import subprocess
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
REFERENCE = {
    "ease": [0, .00448782, .0329515, .0947963, .295244, .408511, .513315,
             .68254, .802403, .885229, .940765, .960459, .975625, .994316,
             .998625, .999946, 1],
    "ease-in-out": [0, .00018982, .00482975, .0197225, .0816599, .129162,
                    .187396, .331884, .5, .668116, .812604, .870838,
                    .91834, .980278, .99517, .99981, 1],
    "cubic-bezier(.3,.8,.3,1)": [0, .0267392, .134946, .271591, .531637,
                                .641421, .73083, .852144, .92041, .959017,
                                .980892, .987811, .992798, .998446,
                                .999637, .999986, 1],
}
TIMES = [0, .01, .05, .1, .2, .25, .3, .4, .5, .6, .7, .75, .8, .9, .95, .99, 1]

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=pathlib.Path, default=ROOT / "target/release/Purr")
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="purr-animation-") as temp:
        report = pathlib.Path(temp) / "curves.json"
        subprocess.run([str(args.binary.resolve()), "--preview", "--animation-check", str(report)], check=True)
        native = json.loads(report.read_text())
    assert len(native["times"]) == len(TIMES)
    assert max(abs(a-b) for a,b in zip(native["times"], TIMES)) < 1e-6
    assert {c["easing"] for c in native["curves"]} == set(REFERENCE)
    errors = []
    for curve in native["curves"]:
        expected = REFERENCE[curve["easing"]]
        assert len(curve["values"]) == len(expected)
        errors.extend(abs(a-b) for a,b in zip(curve["values"], expected))
    maximum = max(errors)
    assert maximum < 0.00001, maximum
    print(f"51 native Slint samples match browser CSS; maximum error {maximum:.9f}")

if __name__ == "__main__":
    main()
