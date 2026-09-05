<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ELF section and segment comparison

Built with `TOS_NUCLEUS_ELF=1`: the same objects and the same linker
script as the production raw image, with the symbol table retained.

| build | tree | raw sha256 | raw bytes |
|---|---|---|---|
| `base-production` | `1c3bb490b1e4` | `4cf4fa35f6ed3e60fb2636b5ba37ccc4…` | 179312 |
| `s4c-production` | `6fc0bf575f93` | `65a95ae8ed45936e64dd0fb8ede4592d…` | 179504 |
| `base-crypto` | `1c3bb490b1e4` | `8ecf7014f3238245694c0d4387a9d27f…` | 134216 |
| `s4c-crypto` | `6fc0bf575f93` | `5dbebfeed572a6fb6cd97e63e6682301…` | 134408 |

## Sections

| section | base-production | s4c-production | delta |
|---|---|---|---|
| `.text` | addr `0x2000000` size `0x1c4af` | addr `0x2000000` size `0x1cfdf` | addr +0, size +2864 |
| `.rodata` | addr `0x201d000` size `0x2211` | addr `0x201d000` size `0x2289` | addr +0, size +120 |
| `.data` | addr `0x2020000` size `0xbbf8` | addr `0x2020000` size `0xbcb8` | addr +0, size +192 |
| `.got` | addr `0x202bbf8` size `0x78` | addr `0x202bcb8` size `0x78` | addr +192, size +0 |
| `.bss` | addr `0x202bc70` size `0xaf50` | addr `0x202bd30` size `0xaf50` | addr +192, size +0 |
| `.comment` | addr `0x0` size `0x8b` | addr `0x0` size `0x8b` | addr +0, size +0 |
| `.symtab` | addr `0x0` size `0x13c8` | addr `0x0` size `0x13e0` | addr +0, size +24 |
| `.shstrtab` | addr `0x0` size `0x42` | addr `0x0` size `0x42` | addr +0, size +0 |
| `.strtab` | addr `0x0` size `0x269a` | addr `0x0` size `0x26cf` | addr +0, size +53 |
