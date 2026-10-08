"""Prepare explicit offline AppContent metadata; SKU is a probe setting, not a license check."""
import argparse
import json
import struct
from pathlib import Path
from prepare import sfo


def prepare(game, out, sku='full'):
    values=sfo((game/'sce_sys/param.sfo').read_bytes())
    params=[values.get(f'USER_DEFINED_PARAM_{i}',0) for i in range(1,5)]
    if any(type(v) is not int or not 0<=v<=0xffffffff for v in params):
        raise ValueError('invalid user-defined parameter')
    profile=dict(title_id=values.get('TITLE_ID'),sku=sku,sku_source='explicit probe setting',
                 user_params=params, mounted_addons=[],boot_attr=0)
    out.mkdir(parents=True,exist_ok=True)
    (out/'content.bin').write_bytes(struct.pack('<8s5I',b'G3CONT01',{'full':3,'trial':1}[sku],*params))
    (out/'content-profile.json').write_text(json.dumps(profile,indent=2)+'\n')
    print(f'AppContent profile: SKU={sku} (probe setting), user params={params}, mounted add-ons=0')

if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('game',type=Path)
    p.add_argument('--out',type=Path,default=Path(__file__).resolve().parent.parent/'out')
    p.add_argument('--sku',choices=['full','trial'],default='full')
    a=p.parse_args();prepare(a.game,a.out,a.sku)
