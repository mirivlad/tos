<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Hot validation and hash path: address and instruction comparison

Symbols on the Stage 1 timed path, from the ELF audit build.

| symbol | base addr | s4c addr | move | instructions | differing |
|---|---|---|---|---|---|
| `…capsule6rd_u64` | `0x200c870` | `0x20072e0` | -21904 | 61 | **0** |
| `…capsule17decode_path_entry` | `0x200c940` | `0x20073b0` | -21904 | 104 | **0** |
| `…capsule17decode_file_entry` | `0x200cae0` | `0x2007550` | -21904 | 101 | **0** |
| `…capsule24update_detached_identity` | `0x200cc30` | `0x20076a0` | -21904 | 26 | **0** |
| `…capsule10check_path` | `0x200cc80` | `0x20076f0` | -21904 | 144 | **0** |
| `…hashNtB4_6Sha2566update` | `0x200d1f0` | `0x2007c60` | -21904 | 84 | **2** |
| `…hashNtB4_6Sha2568finalize` | `0x200d310` | `0x2007d80` | -21904 | 69 | **0** |
| `…hashNtB4_6Sha25614compress_block` | `0x200d400` | `0x2007e70` | -21904 | 179 | **2** |

Every differing instruction is a rip-relative displacement only:

```
  base: call *0x1e9df(%rip) # ADDR <SYM>
  s4c : call *0x24057(%rip) # ADDR <SYM>
  base: call *0x1e948(%rip) # ADDR <SYM>
  s4c : call *0x23fc0(%rip) # ADDR <SYM>
  base: call *0x1e812(%rip) # ADDR <SYM>
  s4c : call *0x23e5a(%rip) # ADDR <SYM>
  base: lea 0x10314(%rip),%r12 # ADDR <SYM>
  s4c : lea 0x15c28(%rip),%r12 # ADDR <SYM>
```

Same opcodes, same registers, same instruction counts. The hot path moved;
it was not recompiled.
