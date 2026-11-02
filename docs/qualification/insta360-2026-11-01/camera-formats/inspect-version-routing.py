"""Read-only, source-bound supplement; does not execute Studio or invoke REA."""
import hashlib
import json
import pathlib
import struct

source = pathlib.Path('/Applications/Insta360 Studio.app/Contents/MacOS/studio-main')
data = source.read_bytes()
digest = hashlib.sha256(data).hexdigest()
assert digest == '6a100040c7e39d202b64e2fc0470e2f52600a39c4cde4e3fb1e92cea5ed76423'
slice_offset = 16384
assert struct.unpack_from('<I', data, slice_offset)[0] == 0xFEEDFACF
position = slice_offset + 32
segments = []
for _ in range(struct.unpack_from('<I', data, slice_offset + 16)[0]):
    command, size = struct.unpack_from('<II', data, position)
    if command == 25:
        va, virtual_size, file_offset, file_size = struct.unpack_from('<QQQQ', data, position + 24)
        segments.append((va, file_offset, file_size))
    position += size
table_va = 0x103C11B48
table_offset = next(slice_offset + offset + table_va - va
                    for va, offset, size in segments if va <= table_va < va + size)
table = data[table_offset:table_offset + 7]
assert table.hex() == '06002d54000086'
def read_at(va, length):
    position = next(slice_offset + offset + va - start
                    for start, offset, size in segments if start <= va < start + size)
    return data[position:position + length]
coefficient_branch = 0x1027A787C + 4 * struct.unpack('<H', read_at(0x103C117B0 + 2 * 70, 2))[0]
fov_branch = 0x1027AA30C + 4 * read_at(0x103C11994 + 70, 1)[0]
static_fov_branch = 0x101B55448 + 4 * read_at(0x103B46067 + 70, 1)[0]
assert coefficient_branch == 0x1027A8090
assert fov_branch == 0x1027AA30C and static_fov_branch == 0x101B55448
coefficients = list(struct.unpack('<4d', read_at(0x103917E00, 32)))
coefficients.append(struct.unpack('<d', struct.pack('<Q', 0xBE4BCF56796544F4))[0])
print(json.dumps({
    'sourceSha256': digest,
    'observation': 'supplemental original-byte static read; not runtime or new REA Evidence',
    'tableVirtualAddress': hex(table_va),
    'tableOriginalFileOffset': hex(table_offset),
    'tableHex': table.hex(),
    'branchBase': '0x1027aa840',
    'versionTargets': {str(i): hex(0x1027AA840 + 4 * entry) for i, entry in enumerate(table)},
    'lens71': {'coefficientBranch': hex(coefficient_branch), 'degreePolynomial': coefficients,
               'coefficientDataAddress': '0x103917e00', 'coefficientImmediateBits': '0xbe4bcf56796544f4',
               'parserFovBranch': hex(fov_branch), 'staticMapFovBranch': hex(static_fov_branch),
               'fovDegrees': struct.unpack('<d', struct.pack('<Q', 0x4069000000000000))[0]},
    'qualification': 'v6 lens193 and legacy dual flags1095/lens71; other branches remain unqualified',
}, indent=2))
