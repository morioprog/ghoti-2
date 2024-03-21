use std::{arch::x86_64::*, mem, simd::*};

use super::BoardOps;
use crate::board::{ENTIRE_HEIGHT, ENTIRE_WIDTH, WIDTH};

#[repr(align(16))]
#[derive(Clone, Copy)]
pub struct BoardBits(pub __m128i);

impl BoardOps for BoardBits {
    fn zero() -> Self {
        unsafe { Self(_mm_setzero_si128()) }
    }

    fn wall() -> Self {
        unsafe {
            mem::transmute(u16x8::from_array([
                0xFFFF, 0x8001, 0x8001, 0x8001, 0x8001, 0x8001, 0x8001, 0xFFFF,
            ]))
        }
    }

    fn board_mask_12() -> Self {
        unsafe {
            mem::transmute(u16x8::from_array([
                0x0000, 0x1FFE, 0x1FFE, 0x1FFE, 0x1FFE, 0x1FFE, 0x1FFE, 0x0000,
            ]))
        }
    }

    fn board_mask_13() -> Self {
        unsafe {
            mem::transmute(u16x8::from_array([
                0x0000, 0x3FFE, 0x3FFE, 0x3FFE, 0x3FFE, 0x3FFE, 0x3FFE, 0x0000,
            ]))
        }
    }

    fn full_mask() -> Self {
        unsafe {
            mem::transmute(u16x8::from_array([
                0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF,
            ]))
        }
    }

    fn onebit(x: usize, y: usize) -> Self {
        debug_assert!(Self::within_bound(x, y));

        let shift = ((x << 4) | y) & 0x3F; // x << 4: choose column by multiplying 16
        let hi = (x as u64) >> 2; // 1 if x >= 4 else 0
        let lo = hi ^ 1;

        unsafe { mem::transmute(u64x2::from_array([lo << shift, hi << shift])) }
    }

    fn andnot(&self, rhs: Self) -> Self {
        Self(unsafe { _mm_andnot_si128(self.0, rhs.0) })
    }

    fn shift_up(&self) -> Self {
        Self(unsafe { _mm_slli_epi16(self.0, 1) })
    }

    fn shift_down(&self) -> Self {
        Self(unsafe { _mm_srli_epi16(self.0, 1) })
    }

    fn shift_left(&self) -> Self {
        Self(unsafe { _mm_srli_si128(self.0, 2) })
    }

    fn shift_right(&self) -> Self {
        Self(unsafe { _mm_slli_si128(self.0, 2) })
    }

    fn popcount(&self) -> usize {
        let lh: i64x2 = self.0.into();
        unsafe { (_popcnt64(lh[0]) + _popcnt64(lh[1])) as usize }
    }

    fn lsb(&self) -> Self {
        debug_assert!(!self.is_zero());

        let lh: u64x2 = self.0.into();
        if lh[0] > 0 {
            return unsafe { mem::transmute(u64x2::from_array([1 << lh[0].trailing_zeros(), 0])) };
        }

        unsafe { mem::transmute(u64x2::from_array([0, 1 << lh[1].trailing_zeros()])) }
    }

    /// Derive LSB for all 16 bit integers.
    fn lsb_u16x8(&self) -> Self {
        // LSB = k & -k = k & (~k + 1)
        let inv = *self ^ Self::full_mask();
        let ones = unsafe { _mm_set1_epi16(1) };
        let minus = Self(unsafe { _mm_add_epi16(inv.0, ones) });

        *self & minus
    }

    fn max_u16x8(&self) -> u16 {
        let inv = *self ^ Self::full_mask();
        let min_inv = unsafe { _mm_minpos_epu16(inv.0) };

        unsafe { !(_mm_cvtsi128_si32(min_inv) as u16) }
    }

    fn pext_u64(a: u64, mask: u64) -> u64 {
        unsafe { _pext_u64(a, mask) }
    }

    fn pdep_u64(a: u64, mask: u64) -> u64 {
        unsafe { _pdep_u64(a, mask) }
    }

    fn before_pop_mask(popped: Self) -> (u64, u64) {
        let lh: u64x2 = (popped ^ Self::full_mask()).0.into();
        (lh[0], lh[1])
    }

    // TODO: AVX512 has _mm_srlv_epi16
    fn after_pop_mask(popped: Self) -> (u64, u64) {
        let lxhx: u64x4 = unsafe {
            let shift = _mm256_cvtepu16_epi32(Self::popcount_u16x8(popped.0));
            let half_ones = _mm256_cvtepu16_epi32(Self::full_mask().0);
            let shifted = _mm256_srlv_epi32(half_ones, shift);
            _mm256_packus_epi32(shifted, shifted).into()
        };
        (lxhx[0], lxhx[2])
    }

    fn get(&self, x: usize, y: usize) -> u8 {
        debug_assert!(Self::within_bound(x, y));

        unsafe { _mm_test_all_zeros(Self::onebit(x, y).0, self.0) as u8 ^ 1 }
    }

    fn set_0(&mut self, x: usize, y: usize) {
        debug_assert!(Self::within_bound(x, y));

        self.0 = Self::onebit(x, y).andnot(*self).0
    }

    fn set_1(&mut self, x: usize, y: usize) {
        debug_assert!(Self::within_bound(x, y));

        self.0 = (*self | Self::onebit(x, y)).0
    }

    fn is_zero(&self) -> bool {
        unsafe { _mm_test_all_zeros(self.0, self.0) != 0 }
    }
}

impl PartialEq<Self> for BoardBits {
    fn eq(&self, other: &Self) -> bool {
        (*self ^ *other).is_zero()
    }
}

impl std::ops::BitAnd for BoardBits {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        Self(unsafe { _mm_and_si128(self.0, rhs.0) })
    }
}

impl std::ops::BitOr for BoardBits {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(unsafe { _mm_or_si128(self.0, rhs.0) })
    }
}

impl std::ops::BitXor for BoardBits {
    type Output = Self;

    fn bitxor(self, rhs: Self) -> Self::Output {
        Self(unsafe { _mm_xor_si128(self.0, rhs.0) })
    }
}

impl From<&'static str> for BoardBits
where
    Self: BoardOps,
{
    fn from(value: &'static str) -> Self {
        debug_assert!(value.len() % WIDTH == 0);

        let mut bb = Self::zero();

        for (_y, chunk) in value.as_bytes().chunks(WIDTH).rev().enumerate() {
            for (_x, c) in chunk.iter().enumerate() {
                if c == &b'1' {
                    // x and y are both one-based
                    bb.set_1(_x + 1, _y + 1);
                }
            }
        }

        bb
    }
}

impl From<(u64, u64)> for BoardBits {
    fn from(value: (u64, u64)) -> Self {
        Self(u64x2::from_array([value.0, value.1]).into())
    }
}

impl Into<(u64, u64)> for BoardBits {
    fn into(self) -> (u64, u64) {
        let lh: u64x2 = self.0.into();
        (lh[0], lh[1])
    }
}

impl BoardBits {
    const fn within_bound(x: usize, y: usize) -> bool {
        x < ENTIRE_WIDTH && y < ENTIRE_HEIGHT
    }

    // TODO: AVX512 has _mm_popcnt_epi16
    unsafe fn popcount_u16x8(a: __m128i) -> __m128i {
        let mask4 = _mm_set1_epi8(0x0F);
        let lookup = _mm_setr_epi8(0, 1, 1, 2, 1, 2, 2, 3, 1, 2, 2, 3, 2, 3, 3, 4);

        let low = _mm_and_si128(mask4, a);
        let high = _mm_and_si128(mask4, _mm_srli_epi16(a, 4));

        let low_count = _mm_shuffle_epi8(lookup, low);
        let high_count = _mm_shuffle_epi8(lookup, high);
        let count8 = _mm_add_epi8(low_count, high_count);

        let count16 = _mm_add_epi8(count8, _mm_slli_epi16(count8, 8));
        _mm_srli_epi16(count16, 8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::{HEIGHT, WIDTH};

    #[test]
    fn constructors() {
        let zero = BoardBits::zero();
        let wall = BoardBits::wall();
        let mask_12 = BoardBits::board_mask_12();
        let mask_13 = BoardBits::board_mask_13();
        let full_mask = BoardBits::full_mask();

        for x in 0..ENTIRE_WIDTH {
            for y in 0..ENTIRE_HEIGHT {
                assert!(
                    zero.get(x, y) == 0,
                    "zero is incorrect at (x: {}, y: {})",
                    x,
                    y
                );
                assert_eq!(
                    wall.get(x, y) == 1,
                    x == 0 || x == ENTIRE_WIDTH - 1 || y == 0 || y == ENTIRE_HEIGHT - 1,
                    "wall is incorrect at (x: {}, y: {})",
                    x,
                    y
                );
                assert_eq!(
                    mask_12.get(x, y) == 1,
                    1 <= x && x <= WIDTH && 1 <= y && y <= HEIGHT,
                    "mask_12 is incorrect at (x: {}, y: {})",
                    x,
                    y
                );
                assert_eq!(
                    mask_13.get(x, y) == 1,
                    1 <= x && x <= WIDTH && 1 <= y && y <= HEIGHT + 1,
                    "mask_13 is incorrect at (x: {}, y: {})",
                    x,
                    y
                );
                assert!(
                    full_mask.get(x, y) == 1,
                    "full_mask is incorrect at (x: {}, y: {})",
                    x,
                    y
                );
            }
        }
    }

    #[test]
    fn from_str() {
        let bb = BoardBits::from(concat!(
            "11..1.", // 2
            ".1...1", // 1
        ));

        assert_eq!(bb.get(1, 1), 0);
        assert_eq!(bb.get(2, 1), 1);
        assert_eq!(bb.get(3, 1), 0);
        assert_eq!(bb.get(4, 1), 0);
        assert_eq!(bb.get(5, 1), 0);
        assert_eq!(bb.get(6, 1), 1);
        assert_eq!(bb.get(1, 2), 1);
        assert_eq!(bb.get(2, 2), 1);
        assert_eq!(bb.get(3, 2), 0);
        assert_eq!(bb.get(4, 2), 0);
        assert_eq!(bb.get(5, 2), 1);
        assert_eq!(bb.get(6, 2), 0);
    }

    #[test]
    fn onebit() {
        assert_eq!(
            BoardBits::onebit(3, 2),
            BoardBits::from(concat!(
                "..1...", // 2
                "......", // 1
            )),
        )
    }

    #[test]
    fn bit_ops() {
        let lhs = BoardBits::from("0011..");
        let rhs = BoardBits::from("0101..");

        assert_eq!(lhs & rhs, BoardBits::from("0001.."));
        assert_eq!(lhs | rhs, BoardBits::from("0111.."));
        assert_eq!(lhs ^ rhs, BoardBits::from("0110.."));
        assert_eq!(lhs.andnot(rhs), BoardBits::from("0100.."))
    }

    #[test]
    fn popcount() {
        assert_eq!(BoardBits::zero().popcount(), 0);
        assert_eq!(BoardBits::onebit(2, 3).popcount(), 1);
        assert_eq!(
            BoardBits::from(concat!(
                "11.1.1", // 2
                ".11.1.", // 1
            ))
            .popcount(),
            7,
        );
    }

    #[test]
    fn lsb() {
        // low
        assert_eq!(
            BoardBits::from(concat!(
                "11.1.1", // 2
                ".11.1.", // 1
            ))
            .lsb(),
            BoardBits::from(concat!(
                "1.....", // 2
                "......", // 1
            ))
        );
        // high
        assert_eq!(
            BoardBits::from(concat!(
                "...1.1", // 2
                "....1.", // 1
            ))
            .lsb(),
            BoardBits::from(concat!(
                "...1..", // 2
                "......", // 1
            ))
        );
    }

    #[test]
    fn lsb_u16x8() {
        let bb: BoardBits = unsafe {
            mem::transmute(u16x8::from_array([
                0x0000, 0x0001, 0x000A, 0x1234, 0xDEAD, 0xE800, 0xF050, 0xFFFF,
            ]))
        };
        let expected: BoardBits = unsafe {
            mem::transmute(u16x8::from_array([
                0x0000, 0x0001, 0x0002, 0x0004, 0x0001, 0x0800, 0x0010, 0x0001,
            ]))
        };

        assert_eq!(bb.lsb_u16x8(), expected);
    }

    #[test]
    fn max_u16x8() {
        let bb1: BoardBits = unsafe {
            mem::transmute(u16x8::from_array([
                0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000,
            ]))
        };
        let bb2: BoardBits = unsafe {
            mem::transmute(u16x8::from_array([
                0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF,
            ]))
        };
        let bb3: BoardBits = unsafe {
            mem::transmute(u16x8::from_array([
                0xABC7, 0xABC3, 0xABC4, 0xABC0, 0xABC6, 0xABC1, 0xABC2, 0xABC5,
            ]))
        };
        let bb4: BoardBits = unsafe {
            mem::transmute(u16x8::from_array([
                0xABCD, 0x0000, 0x0001, 0xFFFF, 0xDEAD, 0xBEEF, 0x000F, 0xFFFE,
            ]))
        };

        assert_eq!(bb1.max_u16x8(), 0x0000);
        assert_eq!(bb2.max_u16x8(), 0xFFFF);
        assert_eq!(bb3.max_u16x8(), 0xABC7);
        assert_eq!(bb4.max_u16x8(), 0xFFFF);
    }

    #[test]
    fn shift_basic() {
        let bb = BoardBits::onebit(2, 3);

        assert_eq!(bb.shift_up(), BoardBits::onebit(2, 4));
        assert_eq!(bb.shift_down(), BoardBits::onebit(2, 2));
        assert_eq!(bb.shift_left(), BoardBits::onebit(1, 3));
        assert_eq!(bb.shift_right(), BoardBits::onebit(3, 3));
    }

    #[test]
    fn shift_corner() {
        assert!(BoardBits::onebit(3, 15).shift_up().is_zero());
        assert!(BoardBits::onebit(3, 0).shift_down().is_zero());
        assert!(BoardBits::onebit(0, 3).shift_left().is_zero());
        assert!(BoardBits::onebit(7, 3).shift_right().is_zero());
    }

    #[test]
    fn get_set() {
        let mut bb = BoardBits::zero();
        assert_eq!(bb.get(2, 3), 0);

        bb.set_1(2, 3);
        assert_eq!(bb.get(2, 3), 1);

        bb.set_0(2, 3);
        assert_eq!(bb.get(2, 3), 0);
    }

    #[test]
    fn is_zero() {
        assert!(BoardBits::zero().is_zero());
        assert!(!BoardBits::onebit(2, 3).is_zero());
    }

    // TODO: add test for pext_u64
    // TODO: add test for pdep_u64
    // TODO: add test for before_pop_mask
    // TODO: add test for after_pop_mask
    // TODO: add test for popcount_u16x8
}
