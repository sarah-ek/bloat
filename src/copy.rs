use super::*;
use equator::assert;

fn copy_finite_with_sign_round(dst: &mut BigFloat, src: &BigFloat, rnd: Round, sign: Sign) -> Approx {
    assert!(src.precision_bits() > dst.precision_bits());
    assert!(src.mantissa().len() >= dst.mantissa().len());

    dst.sign_biased_exponent = ((sign.is_negative() as u64) << consts::SIGN_SHIFT) | (src.sign_biased_exponent & consts::BIASED_EXPONENT_MASK);

    match src.exponent() {
        Exponent::Zero | Exponent::NaN | Exponent::Inf => return Approx::Exact,
        _ => {}
    }

    let limb_diff = src.mantissa().len() - dst.mantissa().len();

    let rnd = rnd.with_sign(sign);

    let one = consts::LIMB_ONE;

    let dst_bits_mod_limb = dst.precision_bits() % consts::LIMB_BITS;
    let msb_pos = consts::LIMB_BITS - dst_bits_mod_limb - 1;

    let (msb, lsb_any) = if dst_bits_mod_limb == 0 {
        let msb = src.mantissa()[limb_diff - 1].shr(msb_pos) == one;
        let mut lsb_any = src.mantissa()[limb_diff - 1] & (one.shl(msb_pos).wrapping_sub(one));
        for &l in src.mantissa()[..limb_diff - 1].iter().rev() {
            lsb_any |= l;
        }
        (msb, lsb_any != consts::LIMB_ZERO)
    } else {
        let msb = (src.mantissa()[limb_diff].shr(msb_pos) & one) == one;
        let mut lsb_any = src.mantissa()[limb_diff] & (one.shl(msb_pos).wrapping_sub(one));
        for &l in src.mantissa()[..limb_diff].iter().rev() {
            lsb_any |= l;
        }
        (msb, lsb_any != consts::LIMB_ZERO)
    };

    if !msb && !lsb_any {
        dst.mantissa_mut().copy_from_slice(&src.mantissa()[limb_diff..]);
        Approx::Exact
    } else {
        let amount = (consts::LIMB_BITS - dst_bits_mod_limb) % consts::LIMB_BITS;
        let ulp = one.shl(amount);
        let mantissa_odd = (src.mantissa()[limb_diff].shr(amount) & one) == one;

        if rnd == RoundKnownSign::AwayFromZero || (rnd == RoundKnownSign::ToNearest && msb && (lsb_any || mantissa_odd)) {
            // round away from zero
            let mut carry;
            let src_m = &src.mantissa()[limb_diff..];

            (dst.mantissa_mut()[0], carry) = (src_m[0] & !(ulp.wrapping_sub(one))).overflowing_add(ulp);

            for (dst, &src) in core::iter::zip(&mut dst.mantissa_mut()[1..], &src_m[1..]) {
                (*dst, carry) = src.overflowing_add(carry as Limb);
            }

            if carry {
                dst.sign_biased_exponent += 1;
                *dst.mantissa_mut().last_mut().unwrap() = one.shl(consts::LIMB_BITS - 1);
            }

            if dst.sign_biased_exponent & consts::BIASED_EXPONENT_MASK == consts::BIASED_EXPONENT_INF {
                Approx::Overflow
            } else {
                Approx::from_sign(sign)
            }
        } else {
            // round to zero
            dst.mantissa_mut().copy_from_slice(&src.mantissa()[limb_diff..]);
            dst.mantissa_mut()[0] &= !(ulp.wrapping_sub(one));

            Approx::from_sign(sign.neg())
        }
    }
}

pub fn copy_with_sign(dst: &mut BigFloat, src: &BigFloat, rnd: Round, sign: Sign) -> Approx {
    dst.sign_biased_exponent = ((sign.is_negative() as u64) << consts::SIGN_SHIFT) | (src.sign_biased_exponent & consts::BIASED_EXPONENT_MASK);
    let approx = if dst.precision_bits() >= src.precision_bits() {
        let len = dst.mantissa_len();
        dst.mantissa_mut()[..len - src.mantissa().len()].fill(consts::LIMB_ZERO);
        dst.mantissa_mut()[len - src.mantissa().len()..].copy_from_slice(&src.mantissa());
        Approx::Exact
    } else {
        copy_finite_with_sign_round(dst, src, rnd, sign)
    };
    approx
}

#[inline]
pub fn copy(dst: &mut BigFloat, src: &BigFloat, rnd: Round) -> Approx {
    copy_with_sign(dst, src, rnd, src.sign())
}

#[inline]
pub fn abs(dst: &mut BigFloat, src: &BigFloat, rnd: Round) -> Approx {
    copy_with_sign(dst, src, rnd, Sign::Pos)
}

#[cfg(test)]
mod tests {
    use super::*;
    use equator::assert;

    #[test]
    fn test_widening_copy() {
        let src = SmallFloat::from_parts(
            4,
            Sign::Neg,
            Exponent::Finite(3),
            utils::rev([0b1011_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000]),
        );
        let mut dst = SmallFloat::from_parts(8, Sign::Pos, Exponent::Zero, utils::rev([0]));

        for &rnd in Round::ALL {
            assert!(copy(&mut dst, &src, rnd) == Approx::Exact);

            assert!(all(
                dst.precision_bits() == 8,
                dst.exponent() == Exponent::Finite(3),
                dst.sign() == Sign::Neg,
                dst.mantissa() == &[0b1011_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000]
            ));
        }
    }

    #[test]
    fn test_very_widening_copy() {
        let src = SmallFloat::from_parts(
            4,
            Sign::Neg,
            Exponent::Finite(3),
            utils::rev([0b1011_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000]),
        );
        let mut dst = SmallFloat::from_parts(72, Sign::Pos, Exponent::Zero, utils::rev([0, 0]));

        for &rnd in Round::ALL {
            assert!(copy(&mut dst, &src, rnd) == Approx::Exact);

            assert!(all(
                dst.precision_bits() == 72,
                dst.exponent() == Exponent::Finite(3),
                dst.sign() == Sign::Neg,
                dst.mantissa()
                    == &utils::rev([
                        0b1011_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000,
                        0b0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000
                    ])
            ));
        }
    }

    #[test]
    fn test_narrowing_copy_exact() {
        let src = SmallFloat::from_parts(
            8,
            Sign::Neg,
            Exponent::Finite(3),
            utils::rev([0b1011_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000]),
        );
        let mut dst = SmallFloat::from_parts(4, Sign::Pos, Exponent::Zero, utils::rev([0, 0]));

        for &rnd in Round::ALL {
            assert!(copy(&mut dst, &src, rnd) == Approx::Exact);

            assert!(all(
                dst.precision_bits() == 4,
                dst.exponent() == Exponent::Finite(3),
                dst.sign() == Sign::Neg,
                dst.mantissa() == &[0b1011_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000]
            ));
        }
    }

    #[test]
    fn test_narrowing_copy_round_nearest_down() {
        let src = SmallFloat::from_parts(
            8,
            Sign::Neg,
            Exponent::Finite(3),
            utils::rev([0b1011_1001_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000]),
        );
        let mut dst = SmallFloat::from_parts(4, Sign::Pos, Exponent::Zero, utils::rev([0, 0]));

        assert!(copy(&mut dst, &src, Round::ToNearest) == Approx::LessThanExact);
        assert!(all(
            dst.precision_bits() == 4,
            dst.exponent() == Exponent::Finite(3),
            dst.sign() == Sign::Neg,
            dst.mantissa() == &[0b1100_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000]
        ));
    }

    #[test]
    fn test_narrowing_copy_round_nearest_down_overflow() {
        let src = SmallFloat::from_parts(
            8,
            Sign::Neg,
            Exponent::Finite(3),
            utils::rev([0b1111_1001_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000]),
        );
        let mut dst = SmallFloat::from_parts(4, Sign::Pos, Exponent::Zero, utils::rev([0, 0]));

        assert!(copy(&mut dst, &src, Round::ToNearest) == Approx::LessThanExact);
        assert!(all(
            dst.precision_bits() == 4,
            dst.exponent() == Exponent::Finite(4),
            dst.sign() == Sign::Neg,
            dst.mantissa() == &[0b1000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000]
        ));
    }

    #[test]
    fn test_narrowing_copy_round_nearest_down_overflow_to_infinity() {
        let src = SmallFloat::from_parts(
            8,
            Sign::Neg,
            Exponent::Finite(consts::MAX_EXPONENT_INCLUSIVE),
            utils::rev([0b1111_1001_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000]),
        );
        let mut dst = SmallFloat::from_parts(4, Sign::Pos, Exponent::Zero, utils::rev([0, 0]));

        assert!(copy(&mut dst, &src, Round::ToNearest) == Approx::Overflow);
        assert!(all(dst.precision_bits() == 4, dst.exponent() == Exponent::Inf, dst.sign() == Sign::Neg,));
    }

    #[test]
    fn test_narrowing_copy_round_nearest_up() {
        let src = SmallFloat::from_parts(
            8,
            Sign::Neg,
            Exponent::Finite(3),
            utils::rev([0b1011_0001_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000]),
        );
        let mut dst = SmallFloat::from_parts(4, Sign::Pos, Exponent::Zero, utils::rev([0, 0]));

        assert!(copy(&mut dst, &src, Round::ToNearest) == Approx::GreaterThanExact);
        assert!(all(
            dst.precision_bits() == 4,
            dst.exponent() == Exponent::Finite(3),
            dst.sign() == Sign::Neg,
            dst.mantissa() == &[0b1011_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000]
        ));
    }

    #[test]
    fn test_narrowing_copy_round_nearest_halfway_down() {
        let src = SmallFloat::from_parts(
            8,
            Sign::Neg,
            Exponent::Finite(3),
            utils::rev([0b1011_1000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000]),
        );
        let mut dst = SmallFloat::from_parts(4, Sign::Pos, Exponent::Zero, utils::rev([0, 0]));

        assert!(copy(&mut dst, &src, Round::ToNearest) == Approx::LessThanExact);
        assert!(all(
            dst.precision_bits() == 4,
            dst.exponent() == Exponent::Finite(3),
            dst.sign() == Sign::Neg,
            dst.mantissa() == &[0b1100_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000]
        ));
    }

    #[test]
    fn test_narrowing_copy_round_nearest_halfway_up() {
        let src = SmallFloat::from_parts(
            8,
            Sign::Neg,
            Exponent::Finite(3),
            utils::rev([0b1010_1000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000]),
        );
        let mut dst = SmallFloat::from_parts(4, Sign::Pos, Exponent::Zero, utils::rev([0, 0]));

        assert!(copy(&mut dst, &src, Round::ToNearest) == Approx::GreaterThanExact);
        assert!(all(
            dst.precision_bits() == 4,
            dst.exponent() == Exponent::Finite(3),
            dst.sign() == Sign::Neg,
            dst.mantissa() == &[0b1010_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000]
        ));
    }

    #[test]
    fn test_narrowing_copy_round_up() {
        let src = SmallFloat::from_parts(
            40,
            Sign::Neg,
            Exponent::Finite(3),
            utils::rev([
                0b1010_1000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000,
                0b1010_1000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000,
            ]),
        );
        let mut dst = SmallFloat::from_parts(4, Sign::Pos, Exponent::Zero, utils::rev([0, 0]));

        assert!(copy(&mut dst, &src, Round::Up) == Approx::GreaterThanExact);
        assert!(all(
            dst.precision_bits() == 4,
            dst.exponent() == Exponent::Finite(3),
            dst.sign() == Sign::Neg,
            dst.mantissa() == &[0b1010_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000]
        ));
    }
}
