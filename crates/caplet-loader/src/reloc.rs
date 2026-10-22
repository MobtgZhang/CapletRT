//! Relocation helpers.
//!
//! The only relocation M3 supports is `R_ARM_RELATIVE`: add the
//! load bias (`actual_base - link_base`) to the 32-bit word at
//! `vaddr`. This is the same relocation `add -fPIC` produces on
//! x86 / Thumb targets, so modules compiled with `pic` work
//! out-of-the-box.

extern crate alloc;

/// Apply a list of relocations to `image`.
///
/// `link_base` is the virtual address the module was linked against
/// (typically 0); `actual_base` is the virtual address the loader
/// placed it at. For each `(vaddr, addend)` pair, the function
/// computes `image[vaddr - link_base] = *(u32*) + addend + (actual_base - link_base)`.
/// For relocatable `.capp` images `link_base` is zero and each relocation
/// address is an offset into the image.
///
/// Returns `Err(RelocError::OutOfRange)` if a vaddr points outside
/// the image.
pub fn apply(
    image: &mut [u8],
    relocs: &[(u32, u32)],
    link_base: u32,
    actual_base: u32,
) -> Result<(), RelocError> {
    let bias = actual_base.wrapping_sub(link_base);
    for &(vaddr, addend) in relocs {
        let off = vaddr.wrapping_sub(link_base) as usize;
        if off + 4 > image.len() {
            return Err(RelocError::OutOfRange);
        }
        let cur = u32::from_le_bytes([image[off], image[off + 1], image[off + 2], image[off + 3]]);
        let next = cur.wrapping_add(addend).wrapping_add(bias);
        let bytes = next.to_le_bytes();
        image[off..off + 4].copy_from_slice(&bytes);
    }
    Ok(())
}

/// Errors emitted by the relocation helper.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelocError {
    /// A relocation's target virtual address is outside the image.
    OutOfRange,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_relative_reloc() {
        let mut img = [0u8; 8];
        // Encode the value 0x1000 at offset 0.
        img[..4].copy_from_slice(&0x1000u32.to_le_bytes());
        // Reloc at image offset 0, addend = 0.
        let relocs = [(0u32, 0u32)];
        apply(&mut img, &relocs, 0, 0x10_0000).unwrap();
        let cur = u32::from_le_bytes([img[0], img[1], img[2], img[3]]);
        // 0x1000 + 0x10_0000 (bias) = 0x10_1000
        assert_eq!(cur, 0x10_1000);
    }

    #[test]
    fn rejects_out_of_range() {
        let mut img = [0u8; 8];
        // Reloc at offset 0x100, far beyond the 8-byte image.
        let relocs = [(0x100u32, 0u32)];
        let r = apply(&mut img, &relocs, 0, 0x10_0000);
        assert!(r.is_err());
    }
}
