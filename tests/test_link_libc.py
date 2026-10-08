"""Cross-module identity and TLS relocation tests without game binaries."""
from paths import ROOT
import json
from pathlib import Path
import struct
import tempfile
import unittest
from unittest.mock import patch
import link_libc
from test_probe import package


class LinkTests(unittest.TestCase):
    def fixture(self, version=1, relocs=()):
        main_key=('fixture',('libc',1),('libc',1))
        lib_key=('fixture',('libc',version),('libc',1))
        main=dict(identity=lambda name:main_key,libraries={'q':('libc',1)},
                  modules={'q':('libc',1)},ph=[dict(type=0x61000001,vaddr=256)])
        symbols=[dict(name='',type=3,binding=0,section=1,value=0,size=0,identity=None),
                 dict(name='fixture#C#A',type=2,binding=1,section=1,value=16,size=1,identity=lib_key)]
        elf=b'\x31\xc0\xc3'+bytes(13)+b'\xc3'+bytes(111)
        libc=dict(elf=elf,ph=[dict(type=1,vaddr=0,memsz=4096,filesz=len(elf),offset=0,flags=5),
                            dict(type=7,vaddr=128,memsz=32,filesz=4)],
                  tags={12:0},symbols=symbols,relocs=list(relocs),sha256='fixture')
        return main,libc

    def run_link(self,main,libc,code=b'\xc3'):
        with tempfile.TemporaryDirectory() as tmp:
            out=Path(tmp)
            (out/'boot.bin').write_bytes(package(code,names=['fixture#q#q'],capabilities=1))
            with patch.object(link_libc,'module',side_effect=[main,libc]):
                link_libc.link(Path('fixture-game'),out)
            return (out/'boot-libc.bin').read_bytes(),json.loads((out/'libc-link.json').read_text())

    def test_different_local_ids_bind_same_identity(self):
        data,report=self.run_link(*self.fixture())
        self.assertEqual(data[:8],b'G3PROBE4')
        self.assertEqual(report['symbol_bindings'],[
            dict(import_name='fixture#q#q',address='0x10010',kind=1)])

    def test_same_nid_wrong_library_version_does_not_bind(self):
        _,report=self.run_link(*self.fixture(version=2))
        self.assertEqual(report['bindings'],0)

    def test_tls_module_id_is_literal_not_image_pointer(self):
        data,report=self.run_link(*self.fixture(relocs=[(64,16,0,0)]))
        size=struct.unpack_from('<Q',data,8)[0]
        image=data[-size:]
        self.assertEqual(struct.unpack_from('<Q',image,65536+64)[0],2)
        self.assertEqual(report['tls_relocations'],1)

    def test_unsupported_tls_symbol_rejected(self):
        with self.assertRaisesRegex(ValueError,'unsupported external TLS'):
            self.run_link(*self.fixture(relocs=[(64,16,1,0)]))

    def test_relocation_outside_segments_rejected(self):
        with self.assertRaisesRegex(ValueError,'relocation outside image'):
            self.run_link(*self.fixture(relocs=[(4092,8,0,0)]))

    def test_fs_thread_pointer_loads_are_rewritten_to_gs(self):
        main,libc=self.fixture()
        load=bytes.fromhex('64488b042500000000')
        code=load+b'\x90'+load+b'\xc3'
        main['ph']=main['ph']+[dict(type=1,vaddr=0,filesz=64,memsz=64,flags=5),
                               dict(type=7,vaddr=2048,filesz=0,memsz=48,align=16)]
        data,report=self.run_link(main,libc,code)
        size=struct.unpack_from('<Q',data,8)[0]
        image=data[-size:]
        self.assertEqual(image[0],0x65)
        self.assertEqual(image[10],0x65)
        self.assertEqual(report['fs_loads_patched'],2)
        self.assertEqual(report['main_tls'],dict(vaddr=2048,filesz=0,memsz=48,align=16))
        self.assertEqual(struct.unpack_from('<4Q',data,56+64),(2048,0,48,16))

    def test_fs_loads_outside_executable_segments_are_kept(self):
        main,libc=self.fixture()
        load=bytes.fromhex('64488b042500000000')
        main['ph']=main['ph']+[dict(type=1,vaddr=0,filesz=64,memsz=64,flags=6)]
        data,report=self.run_link(main,libc,load)
        image=data[-struct.unpack_from('<Q',data,8)[0]:]
        self.assertEqual(image[0],0x64)
        self.assertEqual(report['fs_loads_patched'],0)
