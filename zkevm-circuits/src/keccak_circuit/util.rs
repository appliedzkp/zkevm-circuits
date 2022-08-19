//! Utility traits, functions used in the crate.

use eth_types::Word;

pub(crate) const BIT_COUNT: usize = 3;
pub(crate) const BIT_SIZE: usize = 2usize.pow(BIT_COUNT as u32);

/// PartInfo
#[derive(Clone, Debug)]
pub struct PartInfo {
    /// The bits of the part
    pub bits: Vec<usize>,
}

/// WordParts
#[derive(Clone, Debug)]
pub struct WordParts {
    /// The parts of the word
    pub parts: Vec<PartInfo>,
}

/// Packs bits into bytes
pub mod to_bytes {
    use eth_types::Field;
    use gadgets::util::Expr;
    use halo2_proofs::plonk::Expression;

    pub(crate) fn expr<F: Field>(bits: &[Expression<F>]) -> Vec<Expression<F>> {
        debug_assert!(bits.len() % 8 == 0, "bits not a multiple of 8");

        let mut bytes = Vec::new();
        for byte_bits in bits.chunks(8) {
            let mut value = 0.expr();
            let mut multiplier = F::one();
            for byte in byte_bits.iter() {
                value = value + byte.expr() * multiplier;
                multiplier *= F::from(2);
            }
            bytes.push(value);
        }
        bytes
    }

    pub(crate) fn value(bits: &[u8]) -> Vec<u8> {
        debug_assert!(bits.len() % 8 == 0, "bits not a multiple of 8");

        let mut bytes = Vec::new();
        for byte_bits in bits.chunks(8) {
            let mut value = 0u8;
            for (idx, bit) in byte_bits.iter().enumerate() {
                value += *bit << idx;
            }
            bytes.push(value);
        }
        bytes
    }
}

/// Rotates a word that was split into parts to the right
pub fn rotate<T>(parts: Vec<T>, count: usize, part_size: usize) -> Vec<T> {
    let mut rotated_parts = parts;
    rotated_parts.rotate_right(get_rotate_count(count, part_size));
    rotated_parts
}

/// Rotates a word that was split into parts to the left
pub fn r_rotate<T>(parts: Vec<T>, count: usize, part_size: usize) -> Vec<T> {
    let mut rotated_parts = parts;
    rotated_parts.rotate_left(get_rotate_count(count, part_size));
    rotated_parts
}

/// Encodes the data using rlc
pub mod compose_rlc {
    use eth_types::Field;
    use halo2_proofs::plonk::Expression;

    pub(crate) fn expr<F: Field>(expressions: &[Expression<F>], r: F) -> Expression<F> {
        let mut rlc = expressions[0].clone();
        let mut multiplier = r;
        for expression in expressions[1..].iter() {
            rlc = rlc + expression.clone() * multiplier;
            multiplier *= r;
        }
        rlc
    }
}

/// Scatters a value into a packed word constant
pub mod scatter {
    use super::BIT_SIZE;
    use eth_types::Field;
    use halo2_proofs::plonk::Expression;

    pub(crate) fn expr<F: Field>(value: usize, count: usize) -> Expression<F> {
        let mut packed = F::zero();
        let mut factor = F::one();
        for _ in 0..count {
            packed += F::from(value as u64) * factor;
            factor *= F::from(BIT_SIZE as u64);
        }
        Expression::Constant(packed)
    }
}

/// The words that absorb data
pub fn get_absorb_positions() -> Vec<(usize, usize)> {
    let mut absorb_positions = Vec::new();
    for j in 0..5 {
        for i in 0..5 {
            if i + j * 5 < 17 {
                absorb_positions.push((i, j));
            }
        }
    }
    absorb_positions
}

/// Converts bytes into bits
pub fn into_bits(bytes: &[u8]) -> Vec<u8> {
    let num_bits = bytes.len() * 8;
    let mut bits: Vec<u8> = vec![0; num_bits];

    let mut counter = 0;
    for byte in bytes {
        for idx in 0u64..8 {
            bits[counter] = (*byte >> idx) & 1;
            counter += 1;
        }
    }

    bits
}

/// Converts bits into bytes
pub fn from_bits(bits: &[u8]) -> Word {
    let mut value = Word::zero();
    for (idx, bit) in bits.iter().enumerate() {
        value += Word::from(*bit as u64) << idx;
    }
    value
}

/// Pack bits into a word
pub fn pack(bits: &[u8]) -> Word {
    let mut packed = Word::zero();
    let mut factor = Word::from(1u64);
    for bit in bits {
        packed += Word::from(*bit as u64) * factor;
        factor *= BIT_SIZE;
    }
    packed
}

/// Unpack a word into bits
pub fn unpack(packed: Word) -> [u8; 64] {
    let mut bits = [0; 64];
    for (idx, bit) in bits.iter_mut().enumerate() {
        *bit = ((packed >> (idx * BIT_COUNT)) & Word::from(BIT_SIZE - 1)).as_u32() as u8;
    }
    assert_eq!(pack(&bits), packed);
    bits
}

/// Pack bits stored in a u64 value into a word
pub fn pack_u64(value: u64) -> Word {
    pack(
        &((0..64)
            .map(|i| ((value >> i) & 1) as u8)
            .collect::<Vec<_>>()),
    )
}

/// Normalize bits
pub fn normalize(bits: &[u8]) -> [u8; 64] {
    let mut normalized = [0; 64];
    for (normalized, bit) in normalized.iter_mut().zip(bits.iter()) {
        *normalized = *bit & 1;
    }
    normalized
}

/// Rotates bits left
pub fn rotate_left(bits: &[u8], count: usize) -> [u8; 64] {
    let mut rotated = bits.to_vec();
    rotated.rotate_left(count);
    rotated.try_into().unwrap()
}

/// Gets the target part sizes
pub fn target_part_sizes(part_size: usize) -> Vec<usize> {
    let num_full_chunks = 64 / part_size;
    let partial_chunk_size = 64 % part_size;
    let mut part_sizes = vec![part_size; num_full_chunks];
    if partial_chunk_size > 0 {
        part_sizes.push(partial_chunk_size);
    }
    part_sizes
}

/// Gets the rotation count in parts
pub fn get_rotate_count(count: usize, part_size: usize) -> usize {
    (count + part_size - 1) / part_size
}

/// Gets the word parts
pub fn get_word_parts(part_size: usize, rot: usize, normalize: bool) -> WordParts {
    let mut bits = (0usize..64).collect::<Vec<_>>();
    bits.rotate_right(rot);

    let mut parts = Vec::new();
    let mut rot_idx = 0;

    let mut idx = 0;
    let target_sizes = if normalize {
        // After the rotation we want the parts of all the words to be at the same
        // positions
        target_part_sizes(part_size)
    } else {
        // Here we only care about minimizing the number of parts
        let num_parts_a = rot / part_size;
        let partial_part_a = rot % part_size;

        let num_parts_b = (64 - rot) / part_size;
        let partial_part_b = (64 - rot) % part_size;

        let mut part_sizes = vec![part_size; num_parts_a];
        if partial_part_a > 0 {
            part_sizes.push(partial_part_a);
        }

        part_sizes.extend(vec![part_size; num_parts_b]);
        if partial_part_b > 0 {
            part_sizes.push(partial_part_b);
        }

        part_sizes
    };
    // Split into parts bit by bit
    for part_size in target_sizes {
        let mut num_consumed = 0;
        while num_consumed < part_size {
            let mut part_bits: Vec<usize> = Vec::new();
            while num_consumed < part_size {
                if !part_bits.is_empty() && bits[idx] == 0 {
                    break;
                }
                if bits[idx] == 0 {
                    rot_idx = parts.len();
                }
                part_bits.push(bits[idx]);
                idx += 1;
                num_consumed += 1;
            }
            parts.push(PartInfo { bits: part_bits });
        }
    }

    assert_eq!(get_rotate_count(rot, part_size), rot_idx);

    parts.rotate_left(rot_idx);
    assert_eq!(parts[0].bits[0], 0);

    WordParts { parts }
}
