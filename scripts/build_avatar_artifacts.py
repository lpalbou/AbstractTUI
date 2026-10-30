#!/usr/bin/env python3
"""Build one shared-core Wasm artifact for the sibling UI kit (no publishing)."""
from pathlib import Path
import hashlib,json,subprocess,argparse
ROOT=Path(__file__).resolve().parents[1]
p=argparse.ArgumentParser();p.add_argument('--check',action='store_true');p.add_argument('--ui-kit',type=Path,default=ROOT.parent/'abstractuic/ui-kit');args=p.parse_args()
core=ROOT/'crates/abstract-avatar';h=hashlib.sha256()
for f in [core/'Cargo.toml',*sorted((core/'src').rglob('*.rs'))]:h.update(f.relative_to(core).as_posix().encode());h.update(f.read_bytes())
target=args.ui_kit/'src/avatars';meta=target/'provenance.json'
if args.check:
 v=json.loads(meta.read_text());assert v['source_sha256']==h.hexdigest(),'Shared avatar source changed: rebuild Wasm artifact';assert v['wasm_sha256']==hashlib.sha256((target/'avatar_core.wasm').read_bytes()).hexdigest(),'Wasm hash mismatch'
else:
 subprocess.run(['cargo','build','-p','abstract-avatar','--release','--features','ffi','--target','wasm32-unknown-unknown','--offline'],cwd=ROOT,check=True)
 data=(ROOT/'target/wasm32-unknown-unknown/release/abstract_avatar.wasm').read_bytes();target.mkdir(parents=True,exist_ok=True);(target/'avatar_core.wasm').write_bytes(data);(target/'avatar_core.wasm').chmod(0o644)
 v={'abi':1,'source':'abstracttui/crates/abstract-avatar','crate_version':'0.1.0','source_sha256':h.hexdigest(),'wasm_sha256':hashlib.sha256(data).hexdigest(),'build':'python3 abstracttui/scripts/build_avatar_artifacts.py','rustc':subprocess.check_output(['rustc','--version'],text=True).strip()};meta.write_text(json.dumps(v,indent=2)+'\n')
 print(f'Wrote {len(data):,} bytes to {target}')
