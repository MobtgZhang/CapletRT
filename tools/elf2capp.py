#!/usr/bin/env python3
"""Convert a little-endian ELF executable into a CapletRT .capp container.

The converter loads every PT_LOAD segment into one contiguous image starting
at the lowest segment virtual address. This matches the subset supported by
the CapletRT loader: flat text+data, optional base-relative relocations, and a
32-byte capability descriptor per granted capability.

Examples:
    tools/elf2capp.py app.elf -o app.capp --name app --cap log:3
    tools/elf2capp.py app.elf -o app.capp --non-relocatable --entry 0x60000000
"""

from __future__ import annotations

import argparse
import struct
import sys
from pathlib import Path

MAGIC = b"CAPP"
VERSION = 1
HEADER_SIZE = 120
SIGNATURE_SIZE = 64
FLAG_SIGNED = 1 << 0
FLAG_RELOCATABLE = 1 << 1
PT_LOAD = 1
PT_DYNAMIC = 2
PF_X = 1
PF_W = 2


class ConversionError(Exception):
    pass


def parse_int(value: str) -> int:
    try:
        return int(value, 0)
    except ValueError as exc:
        raise argparse.ArgumentTypeError(f"invalid integer: {value}") from exc


def parse_cap(value: str) -> tuple[str, int]:
    try:
        tag, perms = value.rsplit(":", 1)
        perms_int = int(perms, 0)
    except ValueError as exc:
        raise argparse.ArgumentTypeError(
            f"capability must be TAG:PERMS, got {value!r}"
        ) from exc
    tag_bytes = tag.encode("utf-8")
    if not tag_bytes or len(tag_bytes) > 28:
        raise argparse.ArgumentTypeError("capability tag must be 1..28 UTF-8 bytes")
    if not 0 <= perms_int <= 0xFFFFFFFF:
        raise argparse.ArgumentTypeError("capability permissions must fit in u32")
    return tag, perms_int


def read_elf(path: Path) -> tuple[int, list[tuple[int, bytes, int, int]]]:
    data = path.read_bytes()
    if len(data) < 52 or data[:4] != b"\x7fELF":
        raise ConversionError("not an ELF file")
    elf_class = data[4]
    if data[5] != 1:
        raise ConversionError("only little-endian ELF files are supported")
    if elf_class == 1:
        if len(data) < 52:
            raise ConversionError("truncated ELF32 header")
        entry = struct.unpack_from("<I", data, 24)[0]
        phoff = struct.unpack_from("<I", data, 28)[0]
        phentsize = struct.unpack_from("<H", data, 42)[0]
        phnum = struct.unpack_from("<H", data, 44)[0]
        ph_format = "<IIIIIIII"
        expected_ph_size = 32
    elif elf_class == 2:
        if len(data) < 64:
            raise ConversionError("truncated ELF64 header")
        entry = struct.unpack_from("<Q", data, 24)[0]
        phoff = struct.unpack_from("<Q", data, 32)[0]
        phentsize = struct.unpack_from("<H", data, 54)[0]
        phnum = struct.unpack_from("<H", data, 56)[0]
        ph_format = "<IIQQQQQQ"
        expected_ph_size = 56
    else:
        raise ConversionError(f"unsupported ELF class {elf_class}")

    if phnum == 0:
        raise ConversionError("ELF has no program headers")
    if phentsize < expected_ph_size:
        raise ConversionError("program-header entry is too small")
    if phoff + phentsize * phnum > len(data):
        raise ConversionError("program-header table is outside the file")
    if entry > 0xFFFFFFFF:
        raise ConversionError("entry address does not fit in .capp's u32 field")

    loads: list[tuple[int, bytes, int, int]] = []
    has_dynamic = False
    for index in range(phnum):
        offset = phoff + index * phentsize
        fields = struct.unpack_from(ph_format, data, offset)
        p_type = fields[0]
        if p_type == PT_DYNAMIC:
            has_dynamic = True
        if p_type != PT_LOAD:
            continue
        if elf_class == 1:
            _, file_off, vaddr, _paddr, filesz, memsz, flags, _align = fields
        else:
            _, flags, file_off, vaddr, _paddr, filesz, memsz, _align = fields
        if filesz > memsz:
            raise ConversionError("PT_LOAD file size exceeds memory size")
        if file_off + filesz > len(data):
            raise ConversionError("PT_LOAD data is outside the file")
        if vaddr + memsz > 0x1_0000_0000:
            raise ConversionError("PT_LOAD segment exceeds the 32-bit address space")
        loads.append((vaddr, data[file_off : file_off + filesz], memsz, flags))

    if not loads:
        raise ConversionError("ELF has no loadable PT_LOAD segments")
    if has_dynamic:
        print(
            "elf2capp: warning: PT_DYNAMIC is present; dynamic relocations are "
            "not converted",
            file=sys.stderr,
        )
    return entry, loads


def select_segments(
    loads: list[tuple[int, bytes, int, int]], entry: int, max_gap: int
) -> list[tuple[int, bytes, int, int]]:
    ordered = sorted(loads, key=lambda item: item[0])
    anchor = next(
        (
            index
            for index, (vaddr, _payload, memsz, _flags) in enumerate(ordered)
            if vaddr <= entry < vaddr + memsz
        ),
        None,
    )
    if anchor is None:
        anchor = next(
            (index for index, item in enumerate(ordered) if item[3] & PF_X),
            0,
        )
    selected = {anchor}
    cursor = anchor
    while cursor > 0:
        previous = ordered[cursor - 1]
        current = ordered[cursor]
        if current[0] - (previous[0] + previous[2]) > max_gap:
            break
        cursor -= 1
        selected.add(cursor)
    cursor = anchor
    while cursor + 1 < len(ordered):
        current = ordered[cursor]
        following = ordered[cursor + 1]
        if following[0] - (current[0] + current[2]) > max_gap:
            break
        cursor += 1
        selected.add(cursor)
    result = [ordered[index] for index in sorted(selected)]
    for vaddr, _payload, _memsz, flags in ordered:
        if any(vaddr == item[0] for item in result):
            continue
        if flags & PF_W:
            print(
                f"elf2capp: warning: excluding writable PT_LOAD at 0x{vaddr:x}; "
                "use a single contiguous segment layout for dynamic data",
                file=sys.stderr,
            )
    return result


def build_image(
    loads: list[tuple[int, bytes, int, int]], max_image: int
) -> tuple[int, bytearray]:
    base = min(vaddr for vaddr, _payload, _memsz, _flags in loads)
    end = max(vaddr + memsz for vaddr, _payload, memsz, _flags in loads)
    image_size = end - base
    if image_size <= 0:
        raise ConversionError("empty loadable image")
    if image_size > max_image:
        raise ConversionError(
            f"image would be {image_size} bytes, above --max-image {max_image}"
        )
    image = bytearray(image_size)
    for vaddr, payload, _memsz, _flags in loads:
        start = vaddr - base
        image[start : start + len(payload)] = payload
    return base, image


def encode_capp(
    image: bytearray,
    entry: int,
    stack_size: int,
    priority: int,
    flags: int,
    caps: list[tuple[str, int]],
    name: str,
) -> bytes:
    if not 0 <= stack_size <= 0xFFFFFFFF:
        raise ConversionError("stack size must fit in u32")
    if not 0 <= priority <= 0xFF:
        raise ConversionError("priority must fit in u8")
    if not 0 <= entry <= 0xFFFFFFFF:
        raise ConversionError("entry must fit in u32")

    text_off = HEADER_SIZE
    data_off = text_off + len(image)
    reloc_off = data_off
    cap_off = reloc_off
    name_off = cap_off + len(caps) * 32
    name_bytes = name.encode("utf-8")
    if not name_bytes:
        name_bytes = b"module"
    name_bytes = name_bytes[:32]

    header = bytearray(HEADER_SIZE)
    struct.pack_into("<4sHHIIBBH", header, 0, MAGIC, VERSION, HEADER_SIZE,
                     entry, stack_size, priority, flags, 0)
    struct.pack_into(
        "<IIIIIIII",
        header,
        20,
        text_off,
        len(image),
        data_off,
        0,
        reloc_off,
        0,
        cap_off,
        len(caps),
    )
    struct.pack_into("<I", header, 52, name_off)
    header[56 : 56 + SIGNATURE_SIZE] = b"\x00" * SIGNATURE_SIZE

    out = bytearray(header)
    out.extend(image)
    for tag, perms in caps:
        descriptor = bytearray(32)
        tag_bytes = tag.encode("utf-8")
        descriptor[: len(tag_bytes)] = tag_bytes
        descriptor[28:32] = struct.pack("<I", perms)
        out.extend(descriptor)
    out.extend(name_bytes)
    out.extend(b"\x00" * ((-len(out)) % 4))
    validate_capp(out)
    return bytes(out)


def validate_capp(out: bytes) -> None:
    if len(out) < HEADER_SIZE:
        raise ConversionError("generated container is shorter than its header")
    if out[:4] != MAGIC:
        raise ConversionError("generated container has bad magic")
    version, hdr_size = struct.unpack_from("<HH", out, 4)
    if version != VERSION or hdr_size != HEADER_SIZE:
        raise ConversionError("generated container has an invalid header")
    text_off, text_size, data_off, data_size, reloc_off, reloc_count, cap_off, cap_count = (
        struct.unpack_from("<IIIIIIII", out, 20)
    )
    name_off = struct.unpack_from("<I", out, 52)[0]
    ranges = [
        (text_off, text_size),
        (data_off, data_size),
        (reloc_off, reloc_count * 8),
        (cap_off, cap_count * 32),
        (name_off, len(out) - name_off),
    ]
    for offset, size in ranges:
        if offset < HEADER_SIZE or offset + size > len(out):
            raise ConversionError("generated section lies outside the container")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Convert a little-endian ELF executable to a .capp module"
    )
    parser.add_argument("input", type=Path, help="input ELF file")
    parser.add_argument("-o", "--output", type=Path, required=True, help="output .capp")
    parser.add_argument("--name", default=None, help="module name (default: input stem)")
    parser.add_argument("--entry", type=parse_int, default=None,
                        help="explicit entry value in the .capp header")
    parser.add_argument("--stack", type=parse_int, default=4096, help="stack size")
    parser.add_argument("--priority", type=parse_int, default=8, help="priority 0..255")
    parser.add_argument("--non-relocatable", action="store_true",
                        help="store absolute ELF addresses instead of image offsets")
    parser.add_argument("--cap", action="append", type=parse_cap, default=[],
                        metavar="TAG:PERMS", help="granted capability descriptor")
    parser.add_argument("--max-image", type=parse_int, default=64 * 1024 * 1024,
                        help="reject images larger than this many bytes")
    parser.add_argument("--max-gap", type=parse_int, default=64 * 1024,
                        help="maximum gap between PT_LOAD segments in the flat image")
    args = parser.parse_args(argv)

    try:
        elf_entry, loads = read_elf(args.input)
        selected = select_segments(loads, elf_entry, args.max_gap)
        base, image = build_image(selected, args.max_image)
        relocatable = not args.non_relocatable
        flags = FLAG_RELOCATABLE if relocatable else 0
        if args.entry is not None:
            entry = args.entry
        elif relocatable:
            entry = elf_entry - base
        else:
            entry = elf_entry
        if entry < 0:
            raise ConversionError("ELF entry lies below the loadable image")
        if relocatable and entry >= len(image):
            raise ConversionError(
                f"entry offset 0x{entry:x} is outside the {len(image)}-byte image"
            )
        name = args.name or args.input.stem
        capp = encode_capp(
            image, entry, args.stack, args.priority, flags, args.cap, name
        )
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_bytes(capp)
    except (OSError, ConversionError) as exc:
        print(f"elf2capp: error: {exc}", file=sys.stderr)
        return 1

    print(
        f"elf2capp: wrote {args.output} ({len(capp)} bytes, image={len(image)}, "
        f"base=0x{base:x}, entry=0x{entry:x}, relocatable={relocatable})"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
