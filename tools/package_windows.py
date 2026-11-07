#!/usr/bin/env python3
"""Create a Windows x64 portable ZIP and optional per-user NSIS installer."""
import argparse, hashlib, os, pathlib, shutil, subprocess, urllib.request, zipfile
ROOT = pathlib.Path(__file__).resolve().parents[1]
VERSION = "0.1.0"
CORE_VERSION = "v1.19.33"
CORE_ARCHIVE = "mihomo-windows-amd64-compatible-v1.19.33.zip"
CORE_SHA256 = "7aaf636a4f2172dcd8ec34ac4f52797b059195e5afd6409508c963e545f8864d"
URL = f"https://github.com/MetaCubeX/mihomo/releases/download/{CORE_VERSION}/{CORE_ARCHIVE}"

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=pathlib.Path, default=ROOT/"target/x86_64-pc-windows-msvc/release/Purr.exe")
    parser.add_argument("--installer", action="store_true")
    parser.add_argument("--output-dir", type=pathlib.Path, default=ROOT/"dist")
    args = parser.parse_args()
    if not args.binary.is_file():
        raise SystemExit(f"Build the Windows release first: {args.binary}")
    dist = args.output_dir.resolve()
    cache = ROOT/"target/package-cache"
    cache.mkdir(parents=True, exist_ok=True)
    dist.mkdir(parents=True,exist_ok=True)
    archive = cache/CORE_ARCHIVE
    if not archive.exists():
        with urllib.request.urlopen(URL, timeout=60) as response, archive.open("wb") as output:
            shutil.copyfileobj(response, output)
    if hashlib.sha256(archive.read_bytes()).hexdigest() != CORE_SHA256:
        raise SystemExit("Mihomo SHA-256 mismatch; package was not created")
    bundle = dist/f"Purr-{VERSION}-windows-x64"
    if bundle.exists():
        raise SystemExit(f"Output already exists; choose a new release directory before rebuilding: {bundle}")
    (bundle/"resources").mkdir(parents=True)
    shutil.copy2(args.binary, bundle/"Purr.exe")
    with zipfile.ZipFile(archive) as z:
        exe = [p for p in z.namelist() if p.lower().endswith(".exe")]
        if len(exe) != 1:
            raise SystemExit("Unexpected Mihomo archive layout")
        (bundle/"resources/mihomo-windows-amd64.exe").write_bytes(z.read(exe[0]))
    for geodata in ["GeoIP.dat", "GeoSite.dat", "wintun.dll"]:
        src_geo = ROOT/"resources"/geodata
        if src_geo.is_file():
            shutil.copy2(src_geo, bundle/"resources"/geodata)
    shutil.copy2(ROOT/"LICENSE", bundle/"LICENSE")
    shutil.copy2(ROOT/"vendor/softbuffer/LICENSE-MIT", bundle/"SOFTBUFFER-LICENSE.txt")
    shutil.copy2(ROOT/"docs/verification.md", bundle/"VERIFICATION.md")
    shutil.copy2(ROOT/"docs/memory-options.md", bundle/"memory-options.md")
    (bundle/"README.txt").write_text(
        "PURR for Windows 10/11 x64\n\n"
        "解压整个文件夹后运行 Purr.exe。保留旁边的 resources 文件夹。\n"
        "在“订阅”页粘贴 Clash/Mihomo YAML 订阅链接，点“导入”。\n"
        "按住猫耳或 Space 约一秒连接，按住约 1.6 秒断开。\n"
        "关闭窗口会留在托盘；在托盘选择“退出”会停止内核并恢复原系统代理。\n"
        "可保存、编辑和切换多个订阅，也可拖入 YAML 配置。TUN 需要管理员权限及网络驱动支持。\n\n"
        "本包已交叉编译，尚未在 Windows 实机验证；内存上限尚未达标。\n"
        "详细功能检查、测量结果和限制见 VERIFICATION.md，后续内存方案见 memory-options.md。\n\n"
        "Run Purr.exe. Import a Clash/Mihomo YAML subscription, then hold the cat ears "
        "for one second to connect. Hold again to disconnect.\n"
        "Close hides the window; quit from the tray to restore the previous system proxy.\n"
        "Left or right click the tray icon for the designed control panel.\n"
        "Preferences and profiles: %APPDATA%\\Purr\n"
        "TUN needs an elevated launch and a supported network driver.\n"
        "Standalone visual preview: Purr.exe --preview\n\n"
        "This is an unsigned build. Windows SmartScreen may require More info / Run anyway.\n",
        encoding="utf-8")
    (bundle/"SOURCES.txt").write_text(
        f"Purr {VERSION}: corresponding project source archive accompanies this release.\n"
        f"Mihomo {CORE_VERSION}: https://github.com/MetaCubeX/mihomo/tree/{CORE_VERSION}\n"
        f"Upstream source: https://github.com/MetaCubeX/mihomo/archive/refs/tags/{CORE_VERSION}.tar.gz\n"
        f"Upstream binary archive SHA-256: {CORE_SHA256}\n"
        "Both Purr and Mihomo are GPL-3.0 licensed. See LICENSE.\n"
        "Softbuffer 0.4.8 (MIT): https://github.com/rust-windowing/softbuffer\n"
        "Backport provenance and source: vendor/softbuffer/PATCHES.md in the source archive.\n"
        "See SOFTBUFFER-LICENSE.txt for its copyright and license.\n",
        encoding="utf-8")
    shutil.make_archive(str(bundle), "zip", dist, bundle.name)
    source = dist/f"Purr-{VERSION}-source.zip"
    with zipfile.ZipFile(source,"w",compression=zipfile.ZIP_DEFLATED) as z:
        for base in ["src","tools",".github","docs","vendor"]:
            folder=ROOT/base
            if folder.exists():
                for p in sorted(folder.rglob("*")):
                    if p.is_file() and "__pycache__" not in p.parts and p.name != ".DS_Store":
                        z.write(p, p.relative_to(ROOT))
        for name in ["Cargo.toml","Cargo.lock","LICENSE","README.md"]:
            p=ROOT/name
            if p.exists():z.write(p,name)
    if args.installer:
        compiler=shutil.which("makensis") or shutil.which("makensis.exe")
        if not compiler:
            candidate=pathlib.Path("C:/Program Files (x86)/NSIS/makensis.exe")
            if candidate.is_file():compiler=str(candidate)
        if not compiler:
            raise SystemExit("Portable ZIP ready. Install NSIS (makensis) to build the installer.")
        prefix="/" if os.name=="nt" else "-"
        subprocess.run([compiler,f"{prefix}DAPPDIR={bundle}",f"{prefix}DOUTFILE={dist/f'Purr-{VERSION}-windows-x64-setup.exe'}",str(ROOT/"tools/windows.nsi")],check=True)
    artifacts=[p for p in dist.iterdir() if p.is_file() and p.name!="SHA256SUMS.txt"]
    (dist/"SHA256SUMS.txt").write_text("".join(f"{hashlib.sha256(p.read_bytes()).hexdigest()}  {p.name}\n" for p in sorted(artifacts)),encoding="utf-8")
    print(f"Windows package: {bundle}.zip")
    print(f"Project source: {source}")

if __name__ == "__main__":
    main()
